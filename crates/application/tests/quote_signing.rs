use application::quote_signing::{EnvironmentQuoteSigner, QuoteSigner};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

const TEST_PRIVATE_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[test]
fn signer_signs_a_digest_that_its_public_key_verifies() {
    let signer = EnvironmentQuoteSigner::from_base64url(TEST_PRIVATE_KEY).unwrap();
    let digest = [0xA5; 32];

    let public_key = VerifyingKey::from_bytes(&signer.public_key()).unwrap();
    let signature = Signature::from_bytes(&signer.sign(&digest));

    public_key.verify(&digest, &signature).unwrap();
}

#[test]
fn signer_rejects_tampered_digest_and_signature() {
    let signer = EnvironmentQuoteSigner::from_base64url(TEST_PRIVATE_KEY).unwrap();
    let digest = [0xA5; 32];
    let signature = signer.sign(&digest);
    let mut tampered_digest = digest;
    tampered_digest[0] ^= 1;
    assert!(!signer.verify(&tampered_digest, &signature));
    let mut tampered_signature = signature;
    tampered_signature[0] ^= 1;
    assert!(!signer.verify(&digest, &tampered_signature));
}

#[test]
fn signer_rejects_a_key_with_the_wrong_length() {
    let error = EnvironmentQuoteSigner::from_base64url("AA").unwrap_err();

    assert_eq!(
        error.to_string(),
        "QUOTE_SIGNING_KEY must decode to exactly 32 bytes"
    );
}

#[test]
fn signer_debug_output_never_contains_the_private_key() {
    let signer = EnvironmentQuoteSigner::from_base64url(TEST_PRIVATE_KEY).unwrap();

    assert!(!format!("{signer:?}").contains(TEST_PRIVATE_KEY));
}
