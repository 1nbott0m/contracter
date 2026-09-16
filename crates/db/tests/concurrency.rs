//! Real multi-connection concurrency tests, as opposed to every other
//! integration test in this crate, which runs sequentially inside one
//! transaction. These use two independently pooled connections racing
//! against the same fixture rows to prove idempotency/locking holds under
//! actual concurrent access, not just in a single serialized transaction.
//!
//! Fixture rows here are deliberately committed, not rolled back: two
//! separate connections need to see them. `credit_adjustment_events` /
//! `ledger_transactions` / `ledger_postings` are append-only by an
//! unconditional trigger (`reject_append_only_mutation`, `db/migrations/
//! 0005_append_only_guards.sql`) that blocks UPDATE/DELETE even for the
//! table owner, and the `users`/`ledger_accounts` fixture rows they
//! reference can't be deleted either once referenced by a permanent audit
//! row. There is no cleanup step here by design — this test is expected to
//! run only against a disposable local test database, and the permanent
//! rows it leaves behind are the correct behavior of the append-only ledger
//! this test is verifying, not a test-hygiene bug. The scarcity-publish race
//! test below leaves a similarly permanent `collections`/`catalog_items`/
//! `skus` row behind for the same reason (two real connections need to see
//! it); those rows are not append-only-guarded, just harmless test debris
//! uniquely named per run.

use db::{
    CollectionId, Database, DatabaseConfig, find_current_collection_scarcity,
    post_credit_adjustment, publish_collection_scarcity_snapshot,
};
use uuid::Uuid;

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn concurrent_credit_adjustments_with_the_same_key_settle_exactly_once() {
    let database = test_database().await;

    sqlx::query(
        "INSERT INTO ledger_account_kinds (code, description) VALUES \
         ('system_treasury', 'concurrency test treasury'), \
         ('user_credit', 'concurrency test user credit') \
         ON CONFLICT (code) DO NOTHING",
    )
    .execute(database.pool())
    .await
    .expect("seed ledger account kinds");
    sqlx::query(
        "INSERT INTO ledger_accounts (kind_code) VALUES ('system_treasury') \
         ON CONFLICT DO NOTHING",
    )
    .execute(database.pool())
    .await
    .expect("ensure system treasury");

    let suffix = Uuid::new_v4().simple().to_string();
    let admin_user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("concurrency_admin_{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert admin user");
    let target_user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("concurrency_target_{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert target user");
    let administrator_id: db::AdministratorId =
        sqlx::query_scalar("INSERT INTO administrators (user_id) VALUES ($1) RETURNING id")
            .bind(admin_user_id)
            .fetch_one(database.pool())
            .await
            .expect("insert administrator");
    let target_account_id: db::LedgerAccountId = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (kind_code, owner_user_id) VALUES ('user_credit', $1) \
         RETURNING id",
    )
    .bind(target_user_id)
    .fetch_one(database.pool())
    .await
    .expect("insert target ledger account");
    let target_user_id = db::UserId::new(target_user_id);

    // Two independently pooled connections, not one shared transaction: this
    // is what actually exercises the advisory-lock serialization inside
    // post_credit_adjustment rather than trivially serializing on a
    // single connection.
    let mut connection_a = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection a");
    let mut connection_b = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection b");
    let execution_key = Uuid::new_v4();

    let (result_a, result_b) = tokio::join!(
        post_credit_adjustment(
            &mut connection_a,
            administrator_id,
            target_user_id,
            5_000_000,
            execution_key,
            None,
        ),
        post_credit_adjustment(
            &mut connection_b,
            administrator_id,
            target_user_id,
            5_000_000,
            execution_key,
            None,
        ),
    );

    let transaction_id_a = result_a.expect("first concurrent call settles");
    let transaction_id_b = result_b.expect("second concurrent call settles");
    assert_eq!(
        transaction_id_a, transaction_id_b,
        "two concurrent calls with the same idempotency key must settle on the same \
         ledger transaction, not create two"
    );

    let applied_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM credit_adjustment_events WHERE execution_key = $1",
    )
    .bind(execution_key)
    .fetch_one(database.pool())
    .await
    .expect("count credit adjustment events");
    assert_eq!(
        applied_count, 1,
        "the adjustment must be recorded exactly once under real concurrency, not twice"
    );

    let balance_microcredits: i64 = sqlx::query_scalar(
        "SELECT balance_microcredits FROM ledger_balances WHERE account_id = $1",
    )
    .bind(target_account_id)
    .fetch_one(database.pool())
    .await
    .expect("read ledger balance");
    assert_eq!(
        balance_microcredits, 5_000_000,
        "the balance must reflect exactly one application of the adjustment, not two"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn concurrent_credit_adjustments_with_different_keys_both_settle_independently() {
    let database = test_database().await;

    sqlx::query(
        "INSERT INTO ledger_account_kinds (code, description) VALUES \
         ('system_treasury', 'concurrency test treasury'), \
         ('user_credit', 'concurrency test user credit') \
         ON CONFLICT (code) DO NOTHING",
    )
    .execute(database.pool())
    .await
    .expect("seed ledger account kinds");
    sqlx::query(
        "INSERT INTO ledger_accounts (kind_code) VALUES ('system_treasury') \
         ON CONFLICT DO NOTHING",
    )
    .execute(database.pool())
    .await
    .expect("ensure system treasury");

    let suffix = Uuid::new_v4().simple().to_string();
    let admin_user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("concurrency_admin2_{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert admin user");
    let target_user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("concurrency_target2_{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert target user");
    let administrator_id: db::AdministratorId =
        sqlx::query_scalar("INSERT INTO administrators (user_id) VALUES ($1) RETURNING id")
            .bind(admin_user_id)
            .fetch_one(database.pool())
            .await
            .expect("insert administrator");
    let target_account_id: db::LedgerAccountId = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (kind_code, owner_user_id) VALUES ('user_credit', $1) \
         RETURNING id",
    )
    .bind(target_user_id)
    .fetch_one(database.pool())
    .await
    .expect("insert target ledger account");
    let target_user_id = db::UserId::new(target_user_id);

    // Same two-connection race, but two genuinely different operations
    // (different idempotency keys) on the same account: both must apply
    // exactly once each with a correct final balance. Note this does not by
    // itself prove the advisory lock is scoped to the key rather than the
    // whole account -- `post_credit_adjustment` also takes a `FOR UPDATE`
    // row lock on the ledger account before touching balances, which alone
    // would serialize these two calls correctly regardless of the advisory
    // lock's granularity. This test guards the end-to-end arithmetic under
    // real concurrency, not the lock's key-scoping specifically.
    let mut connection_a = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection a");
    let mut connection_b = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection b");

    let (result_a, result_b) = tokio::join!(
        post_credit_adjustment(
            &mut connection_a,
            administrator_id,
            target_user_id,
            2_000_000,
            Uuid::new_v4(),
            None,
        ),
        post_credit_adjustment(
            &mut connection_b,
            administrator_id,
            target_user_id,
            3_000_000,
            Uuid::new_v4(),
            None,
        ),
    );

    let transaction_id_a = result_a.expect("first concurrent adjustment settles");
    let transaction_id_b = result_b.expect("second concurrent adjustment settles");
    assert_ne!(
        transaction_id_a, transaction_id_b,
        "two distinct idempotency keys must produce two distinct ledger transactions"
    );

    let balance_microcredits: i64 = sqlx::query_scalar(
        "SELECT balance_microcredits FROM ledger_balances WHERE account_id = $1",
    )
    .bind(target_account_id)
    .fetch_one(database.pool())
    .await
    .expect("read ledger balance");
    assert_eq!(
        balance_microcredits, 5_000_000,
        "both distinct concurrent adjustments must be applied exactly once each"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn concurrent_scarcity_publishes_leave_current_pointing_at_the_latest_snapshot() {
    let database = test_database().await;

    // Relies on the seed data's `stock_policy_versions` version 1 (see
    // `db/seeds/0001_reference_data.sql`), which is always active by the
    // time this integration suite runs -- db/verify.sh applies seeds before
    // `cargo test -- --ignored` runs. Using the seeded 'consumer' rarity
    // avoids needing to insert and race a second stock policy version.
    let suffix = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
         VALUES ('concurrency-scarcity', 0, 1, true) ON CONFLICT (code) DO NOTHING",
    )
    .execute(database.pool())
    .await
    .expect("seed wear band");
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Concurrency Scarcity Test') \
         RETURNING id",
    )
    .bind(format!("concurrency-scarcity-{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'consumer', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("concurrency-scarcity-item-{suffix}"))
    .fetch_one(database.pool())
    .await
    .expect("insert catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'concurrency-scarcity' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(database.pool())
    .await
    .expect("insert SKU");
    sqlx::query("INSERT INTO warehouse_stock (sku_id, available_units) VALUES ($1, 40)")
        .bind(sku_id)
        .execute(database.pool())
        .await
        .expect("seed warehouse stock");
    let collection_id = CollectionId::new(collection_id);

    // Two independently pooled connections publishing concurrently: without
    // the pg_advisory_xact_lock in publish_collection_scarcity_snapshot
    // (db/migrations/0011_scarcity_publish_hardening.sql), the two
    // `ON CONFLICT (collection_id) DO UPDATE` upserts into
    // current_collection_scarcity race, and whichever commits last wins --
    // not necessarily the snapshot that was actually created last.
    let mut connection_a = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection a");
    let mut connection_b = database
        .pool()
        .acquire()
        .await
        .expect("acquire connection b");

    let (result_a, result_b) = tokio::join!(
        publish_collection_scarcity_snapshot(&mut *connection_a, "concurrency-scarcity-a"),
        publish_collection_scarcity_snapshot(&mut *connection_b, "concurrency-scarcity-b"),
    );

    let snapshot_id_a = result_a.expect("first concurrent publish settles").get();
    let snapshot_id_b = result_b.expect("second concurrent publish settles").get();
    assert_ne!(
        snapshot_id_a, snapshot_id_b,
        "two concurrent publishes must create two distinct snapshots"
    );
    let latest_snapshot_id = snapshot_id_a.max(snapshot_id_b);

    let current = find_current_collection_scarcity(database.pool(), collection_id)
        .await
        .expect("query current collection scarcity")
        .expect("collection has a current scarcity row");
    assert_eq!(
        current.snapshot_id.get(),
        latest_snapshot_id,
        "current_collection_scarcity must point at the most recently created snapshot, \
         not whichever concurrent publish happened to commit last"
    );
}
