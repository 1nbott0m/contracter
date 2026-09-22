use std::{
    fmt,
    sync::{Arc, OnceLock},
    time::Duration,
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use db::{Database, DatabaseError, PublicId, UserId};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Semaphore;

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
/// How long a request will wait for a password-hashing permit before the
/// server admits it is saturated. Short on purpose: a caller waiting
/// longer than this is already past the point where a useful response is
/// coming, and holding them open only deepens the queue.
///
/// At the default permit count this only fires under genuine saturation:
/// with one permit per core and roughly 60 ms per hash, a five-second
/// queue means a backlog in the thousands.
const DEFAULT_HASHING_QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// The account the session belongs to. Carried out of `login` so the
    /// caller never has to spend a second round trip re-authenticating
    /// the token it was just handed.
    pub user_public_id: PublicId,
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
    #[error("the server is at capacity; retry shortly")]
    Overloaded,
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

    // Check the invitation before paying for the hash.
    //
    // Argon2id costs ~19 MiB and tens of milliseconds by design, and the
    // number that may run at once is capped by `hashing_permits`. Hashing
    // first meant an attacker with no invitation at all could keep that
    // cap saturated with garbage registrations, and every legitimate
    // login would then queue and be shed as 503. This is a probe, not the
    // authorization: `register_invited_user` below still locks the
    // invitation and redeems it atomically, so a caller that wins this
    // check and loses the race still gets `InvitationUnusable`.
    if !db::invitation_is_redeemable(database.pool(), &invitation_token.hash()).await? {
        return Err(AuthError::InvitationUnusable);
    }

    let password_hash = hash_password(password.to_owned()).await?;
    let result = db::register_invited_user(
        database.pool(),
        &invitation_token.hash(),
        login,
        &password_hash,
    )
    .await;

    result.map_err(
        |error| match (error.database_code().as_deref(), error.constraint()) {
            // 23503 unknown invitation, 23514 expired or already redeemed --
            // deliberately collapsed into one client-visible outcome so the
            // response never reveals which invitations exist or their state.
            (Some("23503" | "23514"), _) => AuthError::InvitationUnusable,
            // Only the login index means "that login is taken". The other
            // unique constraints reachable from this function -- one
            // redemption per invitation, one per user -- are a lost race on
            // the invitation, and reporting them as a name clash would tell
            // the caller something false about their chosen login.
            (Some("23505"), Some("users_login_key")) => AuthError::LoginTaken,
            (Some("23505"), _) => AuthError::InvitationUnusable,
            _ => AuthError::Database(error),
        },
    )
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
    // A password outside the accepted length cannot be any account's
    // password, so refusing it before hashing costs the caller nothing and
    // denies an attacker the cheapest way to buy 19 MiB of server work.
    // It is not an oracle: the answer depends only on what the caller
    // typed, never on whether the login exists.
    // The same reasoning applies to the login field, which is otherwise
    // bounded only by the 256 KiB body limit: a 250 KiB login buys a full
    // case-folding comparison in PostgreSQL and then a full Argon2id
    // verification. `InvalidCredentials` rather than `InvalidLogin`, so a
    // malformed login is still indistinguishable from a wrong password.
    if validate_password(password).is_err() || validate_login(login).is_err() {
        return Err(AuthError::InvalidCredentials);
    }

    let credential = db::find_user_credential_by_login(database.pool(), login).await?;

    let (stored_hash, user) = match credential {
        Some(credential) if credential.disabled_at.is_none() => (
            Some(credential.password_hash),
            Some(credential.user_public_id),
        ),
        // Unknown login, or a disabled account: still spend a full
        // verification against a decoy hash before failing.
        _ => (None, None),
    };

    let password_matches = verify_password(password.to_owned(), stored_hash.clone()).await?;
    let (true, Some(user_public_id), Some(verified_hash)) = (password_matches, user, stored_hash)
    else {
        return Err(AuthError::InvalidCredentials);
    };

    let token = SecretToken::generate();
    // The stored hash goes back to the database, which refuses unless it
    // still matches that login's credential. Handing over an internal id
    // instead would let anyone holding the database credential mint a
    // session for any account; see migration 0022.
    let session_public_id = db::create_user_session_for_credential(
        database.pool(),
        login,
        &verified_hash,
        &token.hash(),
        config.session_ttl,
    )
    .await?;

    Ok(IssuedSession {
        token,
        session_public_id,
        user_public_id,
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

/// The caller's own balance, in integer microcredits, derived from the
/// ledger. Never a float, and never another account's balance: the id
/// comes from the session, not from the client.
pub async fn balance(database: &Database, user_public_id: PublicId) -> Result<i64, AuthError> {
    db::find_user_credit_balance(database.pool(), user_public_id)
        .await?
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

/// How many Argon2id operations may be in flight at once.
///
/// Argon2id's cost is the point of it, and that cost is paid in *memory*:
/// the default parameters allocate 19 MiB per operation. Tokio's blocking
/// pool defaults to 512 threads, so an unbounded `spawn_blocking` per
/// login is roughly 9.7 GB of resident memory that any unauthenticated
/// client can ask for, on endpoints with no rate limit. The request
/// timeout does not help: it cancels the *future*, not the blocking task,
/// so an abandoned request keeps its allocation until the hash finishes.
///
/// Bounding it to the machine's parallelism means the memory ceiling is a
/// few hundred megabytes and saturation shows up as a clean `503` instead
/// of the OOM killer. Extra concurrency would not make hashing finish
/// sooner anyway -- there are only so many cores to run it on.
fn hashing_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(2)
        .max(2)
}

fn hashing_permits() -> Arc<Semaphore> {
    static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(PERMITS.get_or_init(|| Arc::new(Semaphore::new(hashing_concurrency()))))
}

static HASHING_QUEUE_TIMEOUT: OnceLock<Duration> = OnceLock::new();

fn hashing_queue_timeout() -> Duration {
    *HASHING_QUEUE_TIMEOUT.get_or_init(|| DEFAULT_HASHING_QUEUE_TIMEOUT)
}

/// Widens the shed threshold for environments that legitimately queue far
/// more concurrent hashes than a served request pattern would.
///
/// The limiter is process-wide, so its timeout is too. This exists for a
/// specific, narrow reason: `cargo test` runs many test binaries in
/// parallel and many tests within each, so an integration suite can ask
/// for hundreds of simultaneous hashes -- a load no rate-limited service
/// would see, but one that trips a threshold tuned for real traffic and
/// turns a correct production behaviour into a flaky test.
///
/// It does not change the permit count, so the memory ceiling that
/// bounds the denial-of-service is untouched. Returns `Err` with the
/// value already in force if hashing has begun, because changing it
/// mid-flight would apply to some waiters and not others.
pub fn set_hashing_queue_timeout(timeout: Duration) -> Result<(), Duration> {
    HASHING_QUEUE_TIMEOUT
        .set(timeout)
        .map_err(|_| hashing_queue_timeout())
}

/// Runs one Argon2id operation, holding a permit for its whole duration.
async fn with_hashing_permit<T, F>(work: F) -> Result<T, AuthError>
where
    F: FnOnce() -> Result<T, AuthError> + Send + 'static,
    T: Send + 'static,
{
    run_permitted(hashing_permits(), work).await
}

/// The body of `with_hashing_permit`, taking its limiter explicitly so a
/// test can exercise saturation against a pool it owns rather than the
/// process-wide one (which every other test in this binary is also using).
///
/// The permit is **moved into the blocking closure**, not held by this
/// future. That distinction is the whole bound.
///
/// A blocking task cannot be cancelled: dropping its `JoinHandle` detaches
/// it and it runs to completion regardless. But dropping this future --
/// which hyper does the instant a client closes its connection, and which
/// the request timeout does on its own schedule -- destroys every local
/// this future owns. An earlier version kept the permit in such a local,
/// so an abandoned request handed its permit straight back while its 19
/// MiB Argon2 kept running. A client that fired requests and closed the
/// socket could therefore hold far more memory than the semaphore admits,
/// which is precisely the denial of service the semaphore was added to
/// prevent. Owned by the closure, the permit survives exactly as long as
/// the allocation it accounts for.
async fn run_permitted<T, F>(permits: Arc<Semaphore>, work: F) -> Result<T, AuthError>
where
    F: FnOnce() -> Result<T, AuthError> + Send + 'static,
    T: Send + 'static,
{
    let permit = tokio::time::timeout(hashing_queue_timeout(), permits.acquire_owned())
        .await
        .map_err(|_| AuthError::Overloaded)?
        .map_err(|_| AuthError::PasswordHashing)?;

    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| AuthError::PasswordHashing)?
}

/// Argon2id is deliberately CPU- and memory-hard, so it runs on the
/// blocking pool: doing it inline would stall an async worker thread for
/// every login and registration.
async fn hash_password(password: String) -> Result<String, AuthError> {
    with_hashing_permit(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AuthError::PasswordHashing)
    })
    .await
}

/// Verifies a password against a stored hash, or, when there is no stored
/// hash to verify against, against a decoy so that an unknown login costs
/// the same as a wrong password. `None` can never return `true`.
async fn verify_password(password: String, stored_hash: Option<String>) -> Result<bool, AuthError> {
    with_hashing_permit(move || {
        // Resolved inside the blocking task on purpose: the first call
        // computes a real Argon2id hash, and doing that on an async worker
        // thread would stall every other task on that thread.
        let hash = stored_hash.unwrap_or_else(|| decoy_password_hash().to_owned());
        let Ok(parsed) = PasswordHash::new(&hash) else {
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
}

/// A real Argon2id hash used so an unknown login still costs a full
/// verification. Computed once.
///
/// Its plaintext is 32 fresh bytes from the OS CSPRNG, discarded
/// immediately, rather than a literal in this file. A literal would be a
/// password that verifies: anyone reading the source could send it and
/// get a `true` out of the verification step. That alone grants nothing
/// here -- `login` also requires a real user, and the decoy path has
/// none -- but it makes the verification's answer meaningless in a way a
/// future caller could easily fail to notice. An unguessable plaintext
/// keeps the property the name promises: nothing matches this hash.
fn decoy_password_hash() -> &'static str {
    static DECOY: OnceLock<String> = OnceLock::new();
    DECOY.get_or_init(|| {
        let mut unguessable = [0_u8; TOKEN_BYTES];
        OsRng.fill_bytes(&mut unguessable);
        let salt = SaltString::generate(&mut OsRng);
        // Built with the same `Argon2::default()` that verification uses,
        // so the decoy costs what a real hash costs. Verification honours
        // the parameters embedded in whatever PHC string it is given, so
        // the moment stored hashes move to stronger parameters -- a bump,
        // or an import -- a decoy pinned to the old ones would verify
        // faster than a real credential and quietly reopen the timing
        // oracle this exists to close. `the_decoy_costs_what_a_real_hash_costs`
        // asserts they still agree.
        Argon2::default()
            .hash_password(&unguessable, &salt)
            .expect("hashing a fixed-width value with default parameters cannot fail")
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
            user_public_id: PublicId::new(uuid::Uuid::nil()),
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
            verify_password(
                "a-sufficiently-long-password".to_owned(),
                Some(hash.clone())
            )
            .await
            .expect("verify")
        );
        assert!(
            !verify_password("a-different-password".to_owned(), Some(hash))
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
            !verify_password("anything".to_owned(), Some("not-a-phc-string".to_owned()))
                .await
                .expect("verification completes")
        );
    }

    #[tokio::test]
    async fn the_decoy_hash_is_a_real_hash_that_nothing_matches() {
        let decoy = decoy_password_hash().to_owned();
        assert!(decoy.starts_with("$argon2id$"), "{decoy}");
        assert!(
            !verify_password("decoy-credential-never-matches".to_owned(), Some(decoy))
                .await
                .expect("verify")
        );
    }

    /// The decoy only equalises timing while it costs what a real hash
    /// costs. Verification uses the parameters embedded in the stored
    /// string, so if those ever diverge from the decoy's, an unknown login
    /// becomes measurably cheaper than a known one.
    #[test]
    fn the_decoy_costs_what_a_real_hash_costs() {
        let decoy = decoy_password_hash();
        let parsed = PasswordHash::new(decoy).expect("the decoy is a valid PHC string");
        let decoy_params = argon2::Params::try_from(&parsed).expect("decoy parameters");
        let current = Argon2::default();

        assert_eq!(parsed.algorithm.as_str(), "argon2id");
        assert_eq!(
            (
                decoy_params.m_cost(),
                decoy_params.t_cost(),
                decoy_params.p_cost()
            ),
            (
                current.params().m_cost(),
                current.params().t_cost(),
                current.params().p_cost()
            ),
            "the decoy must be built with the parameters verification uses"
        );
    }

    /// Abandoning a request must not hand its permit back early.
    ///
    /// A blocking task cannot be cancelled, so the Argon2 keeps running
    /// and keeps its memory. If the permit went back the moment the future
    /// was dropped, a client that fires requests and closes the socket
    /// would hold unbounded memory while the semaphore reported room --
    /// the exact denial of service the semaphore exists to stop.
    ///
    /// Ordered by channels, not by sleeping. An earlier version slept for
    /// fixed durations and asserted in the gaps, which on a machine running
    /// Argon2 on every worker -- as this binary does -- is a test that
    /// eventually fails for reasons unrelated to the behaviour it checks.
    /// Here the blocking work cannot finish until the test says so, so
    /// every assertion is ordered by a happens-before rather than a guess.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_abandoned_request_keeps_its_permit_until_its_hash_finishes() {
        use std::sync::mpsc;

        let permits = Arc::new(Semaphore::new(1));
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (finished_tx, finished_rx) = mpsc::channel::<()>();

        let work = move || {
            started_tx.send(()).expect("the test is listening");
            // Held here until the test has made its assertions.
            release_rx.recv().expect("the test releases the work");
            finished_tx.send(()).expect("the test is listening");
            Ok::<_, AuthError>(())
        };

        let handle = tokio::spawn(run_permitted(Arc::clone(&permits), work));

        // The permit is taken and the blocking work is genuinely under way.
        tokio::task::spawn_blocking(move || started_rx.recv().expect("work started"))
            .await
            .expect("join");

        // Abandon the request, exactly as a closed connection does, and
        // wait until the abort has actually taken effect.
        handle.abort();
        assert!(
            handle.await.expect_err("aborted").is_cancelled(),
            "the request future is gone"
        );

        assert_eq!(
            permits.available_permits(),
            0,
            "an abandoned request must not release its permit while its hash is still running"
        );

        // Only now let the work finish; its permit must come back with it.
        release_tx.send(()).expect("the work is waiting");
        tokio::task::spawn_blocking(move || finished_rx.recv().expect("work finished"))
            .await
            .expect("join");

        // The permit is released as the closure returns, which is a moment
        // after it sends; yield until the runtime has run that drop.
        for _ in 0..1_000 {
            if permits.available_permits() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            permits.available_permits(),
            1,
            "the permit is released when the work it accounts for completes"
        );
    }

    #[tokio::test]
    async fn a_missing_stored_hash_verifies_against_the_decoy_and_never_succeeds() {
        // The absent-credential path must still cost a real verification,
        // and must not be satisfiable by any guess -- including the decoy
        // plaintext itself, which is the one value an attacker who read
        // this file would try.
        for guess in ["", "decoy-credential-never-matches", "anything-at-all"] {
            assert!(
                !verify_password(guess.to_owned(), None)
                    .await
                    .expect("verify"),
                "the decoy must not be matchable by {guess:?}"
            );
        }
    }

    #[tokio::test]
    async fn concurrent_hashing_is_bounded_rather_than_unbounded() {
        // The ceiling is what keeps an unauthenticated flood from turning
        // into ~19 MiB x 512 blocking threads of resident memory.
        let ceiling = hashing_concurrency();
        assert!(ceiling >= 2, "the bound must not deadlock a small machine");
        assert!(
            ceiling <= 64,
            "a bound of {ceiling} is not a bound worth having"
        );
        assert!(
            hashing_permits().available_permits() <= ceiling,
            "the process-wide limiter must be built from that same bound"
        );

        // A pool of its own, so this does not race the other tests in
        // this binary against the process-wide limiter.
        let permits = Arc::new(Semaphore::new(1));
        let held = permits.acquire().await.expect("take the only permit");

        // With every permit held, further work waits instead of being
        // admitted -- which is the whole point: the memory is capped.
        let admitted = tokio::time::timeout(
            Duration::from_millis(200),
            run_permitted(Arc::clone(&permits), || Ok::<_, AuthError>(())),
        )
        .await;
        assert!(
            admitted.is_err(),
            "a saturated pool must make the caller wait, not admit more work"
        );

        // ...and recovers once the pool drains.
        drop(held);
        run_permitted(Arc::clone(&permits), || Ok::<_, AuthError>(()))
            .await
            .expect("a drained pool admits work again");
    }
}
