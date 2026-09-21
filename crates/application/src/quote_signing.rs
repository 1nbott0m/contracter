//! Cryptographic boundary for signing immutable contract quotes.
//!
//! The private signing key is supplied by the server process at startup and
//! never belongs in PostgreSQL, application responses, logs, or source control.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer as _, SigningKey};
use thiserror::Error;

pub trait QuoteSigner: Send + Sync {
    fn public_key(&self) -> [u8; 32];

    fn sign(&self, payload_digest: &[u8; 32]) -> [u8; 64];
}

#[derive(Clone)]
pub struct EnvironmentQuoteSigner {
    signing_key: SigningKey,
}

impl EnvironmentQuoteSigner {
    pub fn from_base64url(encoded_private_key: &str) -> Result<Self, QuoteSigningError> {
        let key_bytes = URL_SAFE_NO_PAD
            .decode(encoded_private_key)
            .map_err(QuoteSigningError::InvalidBase64)?;
        let key_bytes: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| QuoteSigningError::InvalidKeyLength)?;

        Ok(Self {
            signing_key: SigningKey::from_bytes(&key_bytes),
        })
    }
}

impl QuoteSigner for EnvironmentQuoteSigner {
    fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    fn sign(&self, payload_digest: &[u8; 32]) -> [u8; 64] {
        self.signing_key.sign(payload_digest).to_bytes()
    }
}

impl std::fmt::Debug for EnvironmentQuoteSigner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvironmentQuoteSigner")
            .field("public_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum QuoteSigningError {
    #[error("QUOTE_SIGNING_KEY is not valid base64url")]
    InvalidBase64(#[source] base64::DecodeError),
    #[error("QUOTE_SIGNING_KEY must decode to exactly 32 bytes")]
    InvalidKeyLength,
}
