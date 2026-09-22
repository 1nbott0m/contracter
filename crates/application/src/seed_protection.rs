//! Encryption boundary for unrevealed server seeds.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;
use thiserror::Error;

pub struct ProtectedSeed {
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

pub trait SeedProtector: Send + Sync {
    fn encrypt(&self, seed: &[u8; 32]) -> Result<ProtectedSeed, SeedProtectionError>;
    fn decrypt(&self, protected: &ProtectedSeed) -> Result<[u8; 32], SeedProtectionError>;
}

#[derive(Clone)]
pub struct EnvironmentSeedProtector {
    cipher: XChaCha20Poly1305,
}

impl EnvironmentSeedProtector {
    pub fn from_base64url(encoded_key: &str) -> Result<Self, SeedProtectionError> {
        let key = URL_SAFE_NO_PAD
            .decode(encoded_key)
            .map_err(SeedProtectionError::InvalidBase64)?;
        let key: [u8; 32] = key
            .try_into()
            .map_err(|_| SeedProtectionError::InvalidKeyLength)?;
        Ok(Self {
            cipher: XChaCha20Poly1305::new((&key).into()),
        })
    }
}

impl SeedProtector for EnvironmentSeedProtector {
    fn encrypt(&self, seed: &[u8; 32]) -> Result<ProtectedSeed, SeedProtectionError> {
        let mut nonce = [0_u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(XNonce::from_slice(&nonce), seed.as_slice())
            .map_err(|_| SeedProtectionError::Encryption)?;
        Ok(ProtectedSeed { nonce, ciphertext })
    }

    fn decrypt(&self, protected: &ProtectedSeed) -> Result<[u8; 32], SeedProtectionError> {
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(&protected.nonce),
                protected.ciphertext.as_ref(),
            )
            .map_err(|_| SeedProtectionError::Decryption)?;
        plaintext
            .try_into()
            .map_err(|_| SeedProtectionError::Decryption)
    }
}

impl std::fmt::Debug for EnvironmentSeedProtector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("EnvironmentSeedProtector([REDACTED])")
    }
}

#[derive(Debug, Error)]
pub enum SeedProtectionError {
    #[error("QUOTE_SEED_KEY is not valid base64url")]
    InvalidBase64(#[source] base64::DecodeError),
    #[error("QUOTE_SEED_KEY must decode to exactly 32 bytes")]
    InvalidKeyLength,
    #[error("could not encrypt server seed")]
    Encryption,
    #[error("could not decrypt server seed")]
    Decryption,
}
