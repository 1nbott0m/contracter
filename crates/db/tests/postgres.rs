use db::{Database, DatabaseConfig};
use sqlx::Executor;

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn sqlx_applies_migrations_idempotently_and_rolls_back_transactions() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let config = DatabaseConfig::new(url).expect("valid test database URL");
    let database = Database::connect(&config)
        .await
        .expect("connect to isolated PostgreSQL");

    database.migrate().await.expect("first migration run");
    database.migrate().await.expect("idempotent migration run");
    database.health_check().await.expect("health check");

    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations")
        .fetch_one(database.pool())
        .await
        .expect("read SQLx migration history");
    assert_eq!(applied, 19);

    let mut transaction = database.begin().await.expect("begin transaction");
    transaction
        .execute("CREATE TABLE sqlx_transaction_rollback_probe (id integer PRIMARY KEY)")
        .await
        .expect("create rollback probe");
    transaction.rollback().await.expect("rollback transaction");

    let relation: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.sqlx_transaction_rollback_probe')::text")
            .fetch_one(database.pool())
            .await
            .expect("inspect rollback probe");
    assert_eq!(relation, None);
}
