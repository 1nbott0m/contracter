use db::{
    AdministratorId, Database, DatabaseConfig, LedgerAccountId, UserId, post_credit_adjustment,
    publish_collection_scarcity_snapshot, reconcile_current_collection_scarcity,
    reconcile_current_valuations, reconcile_ledger_balances,
    reconcile_published_valuation_snapshots,
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

async fn seed_current_valuation_fixture(
    transaction: &mut Transaction<'_, Postgres>,
) -> (i64, i64, i64) {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) \
             VALUES ('valuation_reconciliation', 93, false) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('valuation_reconciliation', 0, 1, true) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO price_sources (code, display_name, enabled) \
             VALUES ('valuation_reconciliation', 'Valuation reconciliation', true) \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed valuation reconciliation lookups");

    let suffix = Uuid::new_v4().simple().to_string();
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Valuation Reconciliation') \
         RETURNING id",
    )
    .bind(format!("valuation-reconciliation-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'valuation_reconciliation', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("valuation-reconciliation-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'valuation_reconciliation' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation SKU");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) \
         VALUES ('valuation-reconciliation-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation snapshot");
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, \
            window_days, valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ( \
            $1, $2, 1_000_000, 'valuation_reconciliation', 7, 20, clock_timestamp(), \
            decode(repeat('66', 32), 'hex') \
         ) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation snapshot item");
    sqlx::query("UPDATE valuation_snapshots SET published_at = clock_timestamp() WHERE id = $1")
        .bind(snapshot_id)
        .execute(transaction.as_mut())
        .await
        .expect("publish valuation reconciliation snapshot");
    sqlx::query(
        "INSERT INTO current_valuations \
            (sku_id, snapshot_id, snapshot_item_id, verified_price_microcredits) \
         VALUES ($1, $2, $3, 1_000_000)",
    )
    .bind(sku_id)
    .bind(snapshot_id)
    .bind(snapshot_item_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert valuation reconciliation current value");

    (sku_id, snapshot_id, snapshot_item_id)
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

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn valuation_snapshot_reconciliation_detects_a_published_snapshot_without_items() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, healthy_snapshot_id, _) = seed_current_valuation_fixture(&mut transaction).await;
    let healthy = reconcile_published_valuation_snapshots(transaction.as_mut())
        .await
        .expect("reconcile healthy published valuation snapshots");
    assert!(
        healthy
            .iter()
            .all(|row| row.snapshot_id.get() != healthy_snapshot_id),
        "a published valuation snapshot with an item must not be reported"
    );
    transaction
        .execute(
            "ALTER TABLE valuation_snapshots DISABLE TRIGGER valuation_snapshot_requires_items",
        )
        .await
        .expect("temporarily simulate an owner-level integrity bypass");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at, published_at) \
         VALUES ('valuation-reconciliation-corrupt', clock_timestamp(), clock_timestamp()) \
         RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert intentionally malformed published snapshot");
    transaction
        .execute("ALTER TABLE valuation_snapshots ENABLE TRIGGER valuation_snapshot_requires_items")
        .await
        .expect("restore valuation snapshot publication guard");

    let drift = reconcile_published_valuation_snapshots(transaction.as_mut())
        .await
        .expect("reconcile published valuation snapshots");
    assert!(
        drift.iter().any(|row| row.snapshot_id.get() == snapshot_id),
        "a published snapshot without valuation rows must be reported"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn current_valuation_reconciliation_detects_a_cached_price_mismatch() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (sku_id, snapshot_id, snapshot_item_id) =
        seed_current_valuation_fixture(&mut transaction).await;
    let healthy = reconcile_current_valuations(transaction.as_mut())
        .await
        .expect("reconcile untampered current valuation");
    assert!(
        healthy.iter().all(|row| row.sku_id.get() != sku_id),
        "a current valuation copied from its published snapshot item must not be reported"
    );
    sqlx::query(
        "UPDATE current_valuations \
         SET verified_price_microcredits = verified_price_microcredits + 1 \
         WHERE sku_id = $1",
    )
    .bind(sku_id)
    .execute(transaction.as_mut())
    .await
    .expect("tamper with cached valuation price");

    let drift = reconcile_current_valuations(transaction.as_mut())
        .await
        .expect("reconcile current valuations");
    assert!(
        drift.iter().any(|row| {
            row.sku_id.get() == sku_id
                && row.current_snapshot_id.get() == snapshot_id
                && row.current_snapshot_item_id.get() == snapshot_item_id
                && row.source_snapshot_id.get() == snapshot_id
                && row.source_sku_id.get() == sku_id
                && row.cached_price_microcredits == 1_000_001
                && row.source_price_microcredits == 1_000_000
                && row.issue_code == "price_mismatch"
        }),
        "a cached current-valuation price that differs from its snapshot item must be reported"
    );

    transaction.rollback().await.expect("rollback fixture");
}
