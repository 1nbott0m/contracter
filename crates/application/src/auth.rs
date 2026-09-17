use std::{fmt, sync::OnceLock, time::Duration};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use db::{Database, DatabaseError, PublicId, UserId};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Minimum accepted password length. Length, not composition rules, is
/// what modern guidance (NIST SP 800-63B) actually asks for; this is a
/// security floor, not a product decision, and is the only password rule
/// enforced here.
const MINIMUM_PASSWORD_LENGTH: usize = 12;
/// Upper bound so a hostile client cannot make the server hash megabytes
/// of input per request.
const MAXIMUM_PASSWORD_LENGTH: usize = 1024;
/// Matches the `users` table's own CHECK on login length.
const LOGIN_LENGTH: std::ops::RangeInclusive<usize> = 3..=64;
/// 32 bytes of entropy, the same width the schema stores as a hash.
const TOKEN_BYTES: usize = 32;

/// A bearer secret (session or invitation token) in its raw, client-facing
/// form. `Debug` is redacted so it cannot reach a log line by accident,
/// and the value is only ever exposed deliberately via `reveal`.
#[derive(Clone)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Generates a fresh token from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut bytes = [0_u8; TOKEN_BYTES];
        OsRng.fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// The stored form: SHA-256 of the presented token. The raw token
    /// never reaches the database, so a database read cannot reconstruct
    /// a usable session or invitation.
    pub fn hash(&self) -> [u8; 32] {
        Sha256::digest(self.0.as_bytes()).into()
    }

    /// Deliberate, explicit access for the one place that must transmit
    /// the token to its owner.
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretToken([REDACTED])")
    }
}

/// A freshly issued session. The raw token is returned exactly once, to
/// be handed to its owner; only its hash is persisted.
#[derive(Debug)]
pub struct IssuedSession {
    pub token: SecretToken,
    pub session_public_id: PublicId,
    pub ttl: Duration,
}

/// The caller behind an authenticated request, resolved from a session
/// token and nothing else -- never from a client-supplied identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub user_public_id: PublicId,
    pub session_public_id: PublicId,
}

#[derive(Debug, Clone, Copy)]
pub struct AuthConfig {
    pub session_ttl: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl: Duration::from_secs(60 * 60 * 24 * 14),
        }
    }
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("login must be {} to {} characters with no surrounding whitespace", LOGIN_LENGTH.start(), LOGIN_LENGTH.end())]
    InvalidLogin,
    #[error("password must be at least {MINIMUM_PASSWORD_LENGTH} characters")]
    WeakPassword,
    #[error("that login is already taken")]
    LoginTaken,
    #[error("the invitation is not valid")]
    InvitationUnusable,
    #[error("invalid login or password")]
    InvalidCredentials,
    #[error("the session is not valid")]
    SessionInvalid,
    #[error("password hashing failed")]
    PasswordHashing,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Registers an invited user: validates the inputs, derives an Argon2id
/// hash, then redeems the invitation and creates the user in one atomic
/// database call. Returns the new account's public id.
pub async fn register(
    database: &Database,
    invitation_token: &SecretToken,
    login: &str,
    password: &str,
) -> Result<PublicId, AuthError> {
    validate_login(login)?;
    validate_password(password)?;

    let password_hash = hash_password(password.to_owned()).await?;
    let result = db::register_invited_user(
        database.pool(),
        &invitation_token.hash(),
        login,
        &password_hash,
    )
    .await;

    result.map_err(|error| match error.database_code().as_deref() {
        // 23503 unknown invitation, 23514 expired or already redeemed --
        // deliberately collapsed into one client-visible outcome so the
        // response never reveals which invitations exist or their state.
        Some("23503" | "23514") => AuthError::InvitationUnusable,
        Some("23505") => AuthError::LoginTaken,
        _ => AuthError::Database(error),
    })
}

/// Verifies credentials and issues a session. A wrong password and an
/// unknown login are indistinguishable to the caller, and cost the same
/// Argon2id verification either way so timing does not reveal which.
pub async fn login(
    database: &Database,
    config: AuthConfig,
    login: &str,
    password: &str,
) -> Result<IssuedSession, AuthError> {
    let credential = db::find_user_credential_by_login(database.pool(), login).await?;

    let (stored_hash, user) = match credential {
        Some(credential) if credential.disabled_at.is_none() => {
            (credential.password_hash, Some(credential.user_id))
        }
        // Unknown login, or a disabled account: still spend a full
        // verification against a decoy hash before failing.
        _ => (decoy_password_hash().to_owned(), None),
    };

    let password_matches = verify_password(password.to_owned(), stored_hash).await?;
    let (true, Some(user_id)) = (password_matches, user) else {
        return Err(AuthError::InvalidCredentials);
    };

    let token = SecretToken::generate();
    let session_public_id =
        db::create_user_session(database.pool(), user_id, &token.hash(), config.session_ttl)
            .await?;

    Ok(IssuedSession {
        token,
        session_public_id,
        ttl: config.session_ttl,
    })
}

/// Resolves a presented session token to its owner. The only source of
/// request identity.
pub async fn authenticate(
    database: &Database,
    token: &SecretToken,
) -> Result<AuthenticatedUser, AuthError> {
    let session = db::find_active_user_session(database.pool(), &token.hash())
        .await?
        .ok_or(AuthError::SessionInvalid)?;

    Ok(AuthenticatedUser {
        user_id: session.user_id,
        user_public_id: session.user_public_id,
        session_public_id: session.session_public_id,
    })
}

/// Ends one session. Idempotent: logging out twice is not an error.
pub async fn logout(database: &Database, token: &SecretToken) -> Result<(), AuthError> {
    db::revoke_user_session(database.pool(), &token.hash()).await?;
    Ok(())
}

/// Ends every live session for one user, returning how many were revoked.
pub async fn logout_all(database: &Database, user_id: UserId) -> Result<i32, AuthError> {
    Ok(db::revoke_all_user_sessions(database.pool(), user_id).await?)
}

/// The `/me` projection for an already-authenticated caller.
pub async fn account(
    database: &Database,
    user_public_id: PublicId,
) -> Result<db::Account, AuthError> {
    db::find_account_by_public_id(database.pool(), user_public_id)
        .await?
        // An authenticated session whose account no longer resolves means
        // the account was disabled mid-session: treat it as a dead
        // session rather than a missing resource.
        .ok_or(AuthError::SessionInvalid)
}

fn validate_login(login: &str) -> Result<(), AuthError> {
    let trimmed = login.trim();
    if trimmed != login || !LOGIN_LENGTH.contains(&login.chars().count()) {
        return Err(AuthError::InvalidLogin);
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), AuthError> {
    let length = password.chars().count();
    if !(MINIMUM_PASSWORD_LENGTH..=MAXIMUM_PASSWORD_LENGTH).contains(&length) {
        return Err(AuthError::WeakPassword);
    }
    Ok(())
}

/// Argon2id is deliberately CPU- and memory-hard, so it runs on the
/// blocking pool: doing it inline would stall an async worker thread for
/// every login and registration.
async fn hash_password(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AuthError::PasswordHashing)
    })
    .await
    .map_err(|_| AuthError::PasswordHashing)?
}

async fn verify_password(password: String, stored_hash: String) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&stored_hash) else {
            // A stored hash that will not parse is a data problem, not a
            // caller problem: report a failed verification (never a
            // success) and leave the cause to the logs.
            tracing::error!("a stored password hash could not be parsed");
            return Ok(false);
        };
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|_| AuthError::PasswordHashing)?
}

/// A real Argon2id hash of a value no one can log in with, used so an
/// unknown login still costs a full verification. Computed once.
fn decoy_password_hash() -> &'static str {
    static DECOY: OnceLock<String> = OnceLock::new();
    DECOY.get_or_init(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"decoy-credential-never-matches", &salt)
            .expect("hashing a fixed decoy value with default parameters cannot fail")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_token_is_high_entropy_and_hashes_stably() {
        let token = SecretToken::generate();
        assert_ne!(token.reveal(), SecretToken::generate().reveal());
        assert_eq!(token.hash(), token.hash());
        assert_eq!(token.hash().len(), 32);
        assert_ne!(token.hash(), SecretToken::generate().hash());
    }

    #[test]
    fn a_token_never_appears_in_debug_output() {
        let token = SecretToken::new("super-secret-session-value");
        let rendered = format!("{token:?}");
        assert!(
            !rendered.contains("super-secret-session-value"),
            "{rendered}"
        );

        let issued = IssuedSession {
            token,
            session_public_id: PublicId::new(uuid::Uuid::nil()),
            ttl: Duration::from_secs(1),
        };
        let rendered = format!("{issued:?}");
        assert!(
            !rendered.contains("super-secret-session-value"),
            "{rendered}"
        );
    }

    #[test]
    fn login_validation_matches_the_database_constraint() {
        assert!(validate_login("abc").is_ok());
        assert!(validate_login(&"a".repeat(64)).is_ok());
        assert!(validate_login("ab").is_err());
        assert!(validate_login(&"a".repeat(65)).is_err());
        assert!(validate_login(" padded").is_err());
        assert!(validate_login("padded ").is_err());
    }

    #[test]
    fn password_validation_enforces_length_only() {
        assert!(validate_password(&"x".repeat(MINIMUM_PASSWORD_LENGTH)).is_ok());
        assert!(validate_password(&"x".repeat(MINIMUM_PASSWORD_LENGTH - 1)).is_err());
        assert!(validate_password(&"x".repeat(MAXIMUM_PASSWORD_LENGTH + 1)).is_err());
        // No composition rules: a long passphrase is acceptable as-is.
        assert!(validate_password("correct horse battery staple").is_ok());
    }

    #[tokio::test]
    async fn hashing_produces_a_verifiable_argon2id_phc_string() {
        let hash = hash_password("a-sufficiently-long-password".to_owned())
            .await
            .expect("hash");
        assert!(hash.starts_with("$argon2id$"), "{hash}");
        assert!(
            verify_password("a-sufficiently-long-password".to_owned(), hash.clone())
                .await
                .expect("verify")
        );
        assert!(
            !verify_password("a-different-password".to_owned(), hash)
                .await
                .expect("verify")
        );
    }

    #[tokio::test]
    async fn the_same_password_hashes_differently_every_time() {
        let first = hash_password("a-sufficiently-long-password".to_owned())
            .await
            .expect("hash");
        let second = hash_password("a-sufficiently-long-password".to_owned())
            .await
            .expect("hash");
        assert_ne!(first, second, "each hash must use a fresh salt");
    }

    #[tokio::test]
    async fn an_unparsable_stored_hash_fails_verification_instead_of_passing() {
        assert!(
            !verify_password("anything".to_owned(), "not-a-phc-string".to_owned())
                .await
                .expect("verification completes")
        );
    }

    #[tokio::test]
    async fn the_decoy_hash_is_a_real_hash_that_nothing_matches() {
        let decoy = decoy_password_hash().to_owned();
        assert!(decoy.starts_with("$argon2id$"), "{decoy}");
        assert!(
            !verify_password("decoy-credential-never-matches-guess".to_owned(), decoy)
                .await
                .expect("verify")
        );
    }
}
