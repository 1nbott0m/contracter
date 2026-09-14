use db::{DatabaseConfig, DatabaseConfigError};

#[test]
fn debug_output_never_contains_the_connection_url_or_password() {
    let url = [
        "postgres://avnadmin:",
        "test-password",
        "@db.example/defaultdb",
    ]
    .concat();
    let config = DatabaseConfig::new(url).unwrap();
    let output = format!("{config:?}");

    assert!(!output.contains("test-password"));
    assert!(!output.contains("postgres://"));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn invalid_postgres_url_is_rejected_before_connecting() {
    assert!(matches!(
        DatabaseConfig::new("this-is-not-a-postgres-url"),
        Err(DatabaseConfigError::InvalidUrl(_))
    ));
}

#[test]
fn invalid_pool_limits_are_rejected() {
    let config = DatabaseConfig::new("postgres://localhost/defaultdb").unwrap();

    assert!(matches!(
        config.clone().with_pool_limits(0, 0),
        Err(DatabaseConfigError::InvalidPoolLimits)
    ));
    assert!(matches!(
        config.with_pool_limits(4, 2),
        Err(DatabaseConfigError::InvalidPoolLimits)
    ));
}
