use application::seed_protection::{EnvironmentSeedProtector, SeedProtector};

const TEST_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[test]
fn protector_round_trips_a_server_seed_without_exposing_its_key_in_debug_output() {
    let protector = EnvironmentSeedProtector::from_base64url(TEST_KEY).unwrap();
    let seed = [0x5A; 32];

    let protected = protector.encrypt(&seed).unwrap();

    assert_ne!(protected.ciphertext, seed);
    assert_eq!(protector.decrypt(&protected).unwrap(), seed);
    assert!(!format!("{protector:?}").contains(TEST_KEY));
}

#[test]
fn protector_rejects_a_key_with_the_wrong_length() {
    let error = EnvironmentSeedProtector::from_base64url("AA").unwrap_err();

    assert_eq!(
        error.to_string(),
        "QUOTE_SEED_KEY must decode to exactly 32 bytes"
    );
}
