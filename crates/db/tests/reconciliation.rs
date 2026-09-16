use db::{
    AdministratorId, Database, DatabaseConfig, LedgerAccountId, UserId, post_credit_adjustment,
    publish_collection_scarcity_snapshot, reconcile_current_collection_scarcity,
    reconcile_ledger_balances,
};
use sqlx::{Executor, Postgres, Transaction};
use uuid::Uuid;

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

async fn isolated_transaction(database: &Database) -> Transaction<'_, Postgres> {
    let mut transaction = database.begin().await.expect("begin transaction");
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_51)")
        .await
        .expect("serialize reconciliation integration fixtures");
    transaction
}

async fn seed_credit_adjustment_fixture(
    transaction: &mut Transaction<'_, Postgres>,
) -> (AdministratorId, UserId, LedgerAccountId) {
    transaction
        .execute(
            "INSERT INTO ledger_account_kinds (code, description) VALUES \
             ('system_treasury', 'reconciliation test treasury'), \
             ('user_credit', 'reconciliation test user credit') \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed ledger account kinds");
    sqlx::query(
        "INSERT INTO ledger_accounts (kind_code) VALUES ('system_treasury') \
         ON CONFLICT DO NOTHING",
    )
    .execute(transaction.as_mut())
    .await
    .expect("ensure system treasury");

    let suffix = Uuid::new_v4().simple().to_string();
    let admin_user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("reconciliation_admin_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert admin user");
    let target_user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') \
         RETURNING id",
    )
    .bind(format!("reconciliation_target_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert target user");
    let administrator_id: AdministratorId =
        sqlx::query_scalar("INSERT INTO administrators (user_id) VALUES ($1) RETURNING id")
            .bind(admin_user_id)
            .fetch_one(transaction.as_mut())
            .await
            .expect("insert administrator");
    let target_account_id: LedgerAccountId = sqlx::query_scalar(
        "INSERT INTO ledger_accounts (kind_code, owner_user_id) VALUES ('user_credit', $1) \
         RETURNING id",
    )
    .bind(target_user_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert target ledger account");

    (administrator_id, target_user_id, target_account_id)
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn ledger_reconciliation_is_empty_after_a_normal_adjustment() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (administrator_id, target_user_id, account_id) =
        seed_credit_adjustment_fixture(&mut transaction).await;

    post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        7_000_000,
        Uuid::new_v4(),
        None,
    )
    .await
    .expect("post credit adjustment");

    let drift = reconcile_ledger_balances(transaction.as_mut())
        .await
        .expect("reconcile ledger balances");
    assert!(
        !drift.iter().any(|row| row.account_id == account_id),
        "a normally-posted adjustment must leave the cached balance and its \
         postings in agreement"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn ledger_reconciliation_detects_a_direct_balance_tamper() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (administrator_id, target_user_id, account_id) =
        seed_credit_adjustment_fixture(&mut transaction).await;

    post_credit_adjustment(
        transaction.as_mut(),
        administrator_id,
        target_user_id,
        7_000_000,
        Uuid::new_v4(),
        None,
    )
    .await
    .expect("post credit adjustment");

    // Simulates the cached projection silently drifting from its
    // append-only source -- something only a direct owner-level write
    // (never a granted app-role privilege) could do.
    sqlx::query(
        "UPDATE ledger_balances SET balance_microcredits = balance_microcredits + 1 \
         WHERE account_id = $1",
    )
    .bind(account_id)
    .execute(transaction.as_mut())
    .await
    .expect("tamper with cached balance");

    let drift = reconcile_ledger_balances(transaction.as_mut())
        .await
        .expect("reconcile ledger balances");
    let tampered = drift
        .iter()
        .find(|row| row.account_id == account_id)
        .expect("tampered account is reported as drifted");
    assert_eq!(tampered.cached_balance_microcredits, 7_000_001);
    assert_eq!(tampered.posted_balance_microcredits, 7_000_000);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn scarcity_reconciliation_is_empty_after_a_normal_publish() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_43)")
        .await
        .expect("serialize scarcity fixtures against crates/db/tests/scarcity.rs");

    publish_collection_scarcity_snapshot(transaction.as_mut(), "reconciliation-v1")
        .await
        .expect("publish scarcity snapshot");

    let drift = reconcile_current_collection_scarcity(transaction.as_mut())
        .await
        .expect("reconcile current collection scarcity");
    assert!(
        drift.is_empty(),
        "a normal publish must leave every collection's current pointer at \
         its own latest snapshot"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn scarcity_reconciliation_detects_a_stale_current_pointer() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_43)")
        .await
        .expect("serialize scarcity fixtures against crates/db/tests/scarcity.rs");

    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Reconciliation Test') \
         RETURNING id",
    )
    .bind(format!("reconciliation-{}", Uuid::new_v4().simple()))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    sqlx::query(
        "INSERT INTO rarities (code, rank, is_covert) VALUES ('reconciliation-test', 94, false) \
         ON CONFLICT (code) DO NOTHING",
    )
    .execute(transaction.as_mut())
    .await
    .expect("seed rarity");
    transaction
        .execute(
            "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('reconciliation-test', 0, 1, true) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed wear band");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'reconciliation-test', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("reconciliation-item-{}", Uuid::new_v4().simple()))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'reconciliation-test' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU");
    let stock_policy_version_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) \
         VALUES ($1, clock_timestamp()) RETURNING id",
    )
    .bind((Uuid::new_v4().as_u128() % 1_000_000) as i32 + 1)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert active stock policy version");
    sqlx::query(
        "INSERT INTO stock_policy_bands \
            (stock_policy_version_id, rarity_code, minimum_units, target_units, maximum_units) \
         VALUES ($1, 'reconciliation-test', 0, 10, 10)",
    )
    .bind(stock_policy_version_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert stock policy band");
    sqlx::query("INSERT INTO warehouse_stock (sku_id, available_units) VALUES ($1, 5)")
        .bind(sku_id)
        .execute(transaction.as_mut())
        .await
        .expect("seed warehouse stock");

    let first_snapshot_id =
        publish_collection_scarcity_snapshot(transaction.as_mut(), "reconciliation-v1")
            .await
            .expect("publish first scarcity snapshot");
    publish_collection_scarcity_snapshot(transaction.as_mut(), "reconciliation-v2")
        .await
        .expect("publish second scarcity snapshot");

    // Simulates exactly the race BLOCKED_DECISIONS.md #2/the earlier
    // concurrent-publish finding describes: current_collection_scarcity
    // pointing at an older snapshot than the append-only history's latest
    // row. Only a direct owner-level write (never a granted app-role
    // privilege) could do this.
    sqlx::query("UPDATE current_collection_scarcity SET snapshot_id = $1 WHERE collection_id = $2")
        .bind(first_snapshot_id)
        .bind(collection_id)
        .execute(transaction.as_mut())
        .await
        .expect("simulate a stale current pointer");

    let drift = reconcile_current_collection_scarcity(transaction.as_mut())
        .await
        .expect("reconcile current collection scarcity");
    let stale = drift
        .iter()
        .find(|row| row.collection_id.get() == collection_id)
        .expect("stale collection is reported as drifted");
    assert_eq!(stale.current_snapshot_id, first_snapshot_id);
    assert_ne!(stale.latest_published_snapshot_id, first_snapshot_id);

    transaction.rollback().await.expect("rollback fixture");
}
