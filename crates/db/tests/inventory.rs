use db::{
    Database, DatabaseConfig, InventoryItemId, PublicId, SkuId, UserId,
    chrono::{DateTime, Utc},
    find_inventory_item, list_available_user_inventory, list_available_warehouse_inventory,
};
use rust_decimal::Decimal;
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_15)")
        .await
        .expect("serialize inventory integration fixtures");
    transaction
}

async fn seed_inventory_item(
    transaction: &mut Transaction<'_, Postgres>,
) -> (InventoryItemId, PublicId, SkuId, UserId) {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) \
             VALUES ('inventory_test', 90, false) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed rarity");
    transaction
        .execute(
            "INSERT INTO wear_bands \
                (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('inventory_test', 0, 1, true) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed wear band");

    let suffix = Uuid::new_v4().simple().to_string();
    let user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') RETURNING id",
    )
    .bind(format!("inventory_user_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert user");
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Inventory Test') RETURNING id",
    )
    .bind(format!("inventory-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'inventory_test', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("inventory-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    let sku_id: SkuId = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'inventory_test' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU");
    let public_id = PublicId::new(Uuid::new_v4());
    let inventory_item_id: InventoryItemId = sqlx::query_scalar(
        "INSERT INTO inventory_items (public_id, sku_id, canonical_float) \
         VALUES ($1, $2, 0.12345678) RETURNING id",
    )
    .bind(public_id)
    .bind(sku_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert inventory item");
    sqlx::query(
        "INSERT INTO inventory_positions (inventory_item_id, owner_user_id) VALUES ($1, $2)",
    )
    .bind(inventory_item_id)
    .bind(user_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert inventory position");

    (inventory_item_id, public_id, sku_id, user_id)
}

async fn insert_inventory_item(
    transaction: &mut Transaction<'_, Postgres>,
    sku_id: SkuId,
    canonical_float: Decimal,
    owner_user_id: Option<UserId>,
    in_warehouse: bool,
    retired: bool,
) -> (InventoryItemId, PublicId) {
    let public_id = PublicId::new(Uuid::new_v4());
    let item_id: InventoryItemId = sqlx::query_scalar(
        "INSERT INTO inventory_items \
            (public_id, sku_id, canonical_float, retired_at) \
         VALUES ($1, $2, $3, \
                 CASE WHEN $4 THEN clock_timestamp() ELSE NULL END) \
         RETURNING id",
    )
    .bind(public_id)
    .bind(sku_id)
    .bind(canonical_float)
    .bind(retired)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert inventory item");
    sqlx::query(
        "INSERT INTO inventory_positions \
            (inventory_item_id, owner_user_id, in_warehouse) \
         VALUES ($1, $2, $3)",
    )
    .bind(item_id)
    .bind(owner_user_id)
    .bind(in_warehouse)
    .execute(transaction.as_mut())
    .await
    .expect("insert inventory position");
    (item_id, public_id)
}

async fn seed_quote(transaction: &mut Transaction<'_, Postgres>, user_id: UserId) -> (i64, i64) {
    transaction
        .execute(
            "INSERT INTO quote_statuses (code, is_terminal, description) \
             VALUES ('active', false, 'test active') ON CONFLICT (code) DO NOTHING; \
             INSERT INTO price_sources (code, display_name) \
             VALUES ('inventory_test', 'Inventory test') ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed quote lookups");

    let commitment_id: i64 = sqlx::query_scalar(
        "INSERT INTO seed_commitments \
            (sequence_number, commitment_hash, encoding_version) \
         VALUES (900001, decode(repeat('11', 32), 'hex'), 'test-v1') RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert seed commitment");
    // clock_timestamp() is volatile and can return a different instant on
    // each call within the same statement, so allocated_at's column DEFAULT
    // and an independent `expires_at` expression can drift by a few
    // microseconds and intermittently violate the "expires_at <=
    // allocated_at + 15s" CHECK. Capture one reading and derive both
    // columns from it instead of relying on two separate clock reads.
    let allocation_id: i64 = sqlx::query_scalar(
        "WITH allocation_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO seed_allocations (commitment_id, user_id, allocated_at, expires_at) \
         SELECT $1, $2, allocation_clock.now, allocation_clock.now + interval '15 seconds' \
         FROM allocation_clock \
         RETURNING id",
    )
    .bind(commitment_id)
    .bind(user_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert seed allocation");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) \
         VALUES ('inventory-test-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation snapshot");
    let stock_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) \
         VALUES (900001, clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert stock policy");
    let risk_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO risk_policy_versions ( \
            version, minimum_coverage_ratio, maximum_quote_reserve_ratio, \
            maximum_item_liability_ratio, maximum_collection_liability_ratio, \
            minimum_notional_microcredits, maximum_dispersion_ratio, activated_at \
         ) VALUES (900001, 1, 0.1, 0.1, 0.2, 0, 1, clock_timestamp()) \
         RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert risk policy");
    let signing_key_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_signing_keys (public_key, activated_at) \
         VALUES (decode(repeat('22', 32), 'hex'), clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert signing key");

    let quote_id = sqlx::query_scalar(
        "INSERT INTO tradeup_quotes ( \
            user_id, allocation_id, commitment_id, valuation_snapshot_id, \
            stock_policy_version_id, risk_policy_version_id, signing_key_id, \
            status_code, formula_version, client_seed, nonce, \
            verified_input_value_microcredits, expected_buyback_microcredits, \
            quote_total_microcredits, adjustment_microcredits, \
            maximum_exposure_microcredits, ordered_outcome_digest, signature, \
            expires_at \
         ) VALUES ( \
            $1, $2, $3, $4, $5, $6, $7, 'active', 'inventory-test-v1', \
            decode(repeat('33', 32), 'hex'), 0, 0, 0, 0, 0, 0, \
            decode(repeat('44', 32), 'hex'), decode(repeat('55', 64), 'hex'), \
            clock_timestamp() + interval '30 seconds' \
         ) RETURNING id",
    )
    .bind(user_id)
    .bind(allocation_id)
    .bind(commitment_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .bind(risk_policy_id)
    .bind(signing_key_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert active quote");

    (quote_id, snapshot_id)
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn inventory_item_is_loaded_with_exact_float_and_current_position() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (item_id, public_id, sku_id, user_id) = seed_inventory_item(&mut transaction).await;

    let item = find_inventory_item(transaction.as_mut(), public_id)
        .await
        .expect("read inventory item")
        .expect("inventory item exists");

    assert_eq!(item.id, item_id);
    assert_eq!(item.sku_id, sku_id);
    assert_eq!(item.canonical_float, Decimal::new(12_345_678, 8));
    assert_eq!(item.owner_user_id, Some(user_id));
    assert!(!item.in_warehouse);
    assert_eq!(item.position_version, 1);
    assert!(item.retired_at.is_none());

    sqlx::query(
        "UPDATE inventory_positions \
         SET owner_user_id = NULL, in_warehouse = true, version = 2 \
         WHERE inventory_item_id = $1",
    )
    .bind(item_id)
    .execute(transaction.as_mut())
    .await
    .expect("move inventory item to warehouse");
    let moved = find_inventory_item(transaction.as_mut(), public_id)
        .await
        .expect("read moved inventory item")
        .expect("moved inventory item exists");
    assert_eq!(moved.owner_user_id, None);
    assert!(moved.in_warehouse);
    assert_eq!(moved.position_version, 2);

    assert!(
        find_inventory_item(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("read unknown inventory item")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn user_inventory_excludes_retired_and_actively_locked_items_in_stable_order() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (first_id, _, sku_id, user_id) = seed_inventory_item(&mut transaction).await;
    let (second_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(20_000_000, 8),
        Some(user_id),
        false,
        false,
    )
    .await;
    let (locked_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(30_000_000, 8),
        Some(user_id),
        false,
        false,
    )
    .await;
    let (expired_lock_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(35_000_000, 8),
        Some(user_id),
        false,
        false,
    )
    .await;
    insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(40_000_000, 8),
        Some(user_id),
        false,
        true,
    )
    .await;
    let (quote_id, _) = seed_quote(&mut transaction, user_id).await;
    sqlx::query(
        "INSERT INTO inventory_item_locks \
            (inventory_item_id, quote_id, expires_at) \
         VALUES ($1, $2, clock_timestamp() + interval '30 seconds')",
    )
    .bind(locked_id)
    .bind(quote_id)
    .execute(transaction.as_mut())
    .await
    .expect("lock inventory item");
    sqlx::query(
        "INSERT INTO inventory_item_locks \
            (inventory_item_id, quote_id, locked_at, expires_at) \
         VALUES ($1, $2, clock_timestamp() - interval '2 minutes', \
                 clock_timestamp() - interval '1 minute')",
    )
    .bind(expired_lock_id)
    .bind(quote_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert expired inventory lock");

    let items = list_available_user_inventory(transaction.as_mut(), user_id)
        .await
        .expect("list available user inventory");

    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![first_id, second_id, expired_lock_id]
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn warehouse_inventory_excludes_retired_locked_and_reserved_items_in_stable_order() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, _, sku_id, user_id) = seed_inventory_item(&mut transaction).await;
    let (first_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(50_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    let (second_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(60_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    let (reserved_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(70_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    let (locked_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(80_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    let (expired_lock_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(85_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    let (expired_reservation_id, _) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(87_000_000, 8),
        None,
        true,
        false,
    )
    .await;
    insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(90_000_000, 8),
        None,
        true,
        true,
    )
    .await;

    let (quote_id, snapshot_id) = seed_quote(&mut transaction, user_id).await;
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, \
            window_days, valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ( \
            $1, $2, 1000000, 'inventory_test', 7, 20, clock_timestamp(), \
            decode(repeat('66', 32), 'hex') \
         ) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation snapshot item");
    let outcome_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_outcomes ( \
            quote_id, position, sku_id, candidate_inventory_item_id, \
            valuation_snapshot_item_id, probability_numerator, \
            probability_denominator, output_float, buyback_microcredits \
         ) VALUES ($1, 1, $2, $3, $4, 1, 1, 0.70000000, 0) RETURNING id",
    )
    .bind(quote_id)
    .bind(sku_id)
    .bind(reserved_id)
    .bind(snapshot_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote outcome");
    sqlx::query(
        "INSERT INTO quote_candidate_reservations ( \
            quote_id, quote_outcome_id, inventory_item_id, sku_id, reserved_until \
         ) VALUES ($1, $2, $3, $4, clock_timestamp() + interval '30 seconds')",
    )
    .bind(quote_id)
    .bind(outcome_id)
    .bind(reserved_id)
    .bind(sku_id)
    .execute(transaction.as_mut())
    .await
    .expect("reserve warehouse candidate");
    let expired_outcome_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_outcomes ( \
            quote_id, position, sku_id, candidate_inventory_item_id, \
            valuation_snapshot_item_id, probability_numerator, \
            probability_denominator, output_float, buyback_microcredits \
         ) VALUES ($1, 2, $2, $3, $4, 1, 1, 0.70000000, 0) RETURNING id",
    )
    .bind(quote_id)
    .bind(sku_id)
    .bind(expired_reservation_id)
    .bind(snapshot_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert expired reservation outcome");
    sqlx::query(
        "INSERT INTO quote_candidate_reservations ( \
            quote_id, quote_outcome_id, inventory_item_id, sku_id, reserved_until \
         ) VALUES ($1, $2, $3, $4, clock_timestamp() - interval '1 minute')",
    )
    .bind(quote_id)
    .bind(expired_outcome_id)
    .bind(expired_reservation_id)
    .bind(sku_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert expired warehouse reservation");
    sqlx::query(
        "INSERT INTO inventory_item_locks \
            (inventory_item_id, quote_id, expires_at) \
         VALUES ($1, $2, clock_timestamp() + interval '30 seconds')",
    )
    .bind(locked_id)
    .bind(quote_id)
    .execute(transaction.as_mut())
    .await
    .expect("lock warehouse inventory item");
    sqlx::query(
        "INSERT INTO inventory_item_locks \
            (inventory_item_id, quote_id, locked_at, expires_at) \
         VALUES ($1, $2, clock_timestamp() - interval '2 minutes', \
                 clock_timestamp() - interval '1 minute')",
    )
    .bind(expired_lock_id)
    .bind(quote_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert expired warehouse lock");

    let items = list_available_warehouse_inventory(transaction.as_mut())
        .await
        .expect("list available warehouse inventory");

    assert_eq!(
        items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![first_id, second_id, expired_lock_id, expired_reservation_id]
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_query_availability_without_guard_table_access() {
    let database = test_database().await;
    let runtime_role_required = std::env::var_os("TEST_RUNTIME_ROLE_REQUIRED").is_some();
    let role_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')",
    )
    .fetch_one(database.pool())
    .await
    .expect("inspect runtime role");
    if !role_exists {
        assert!(
            !runtime_role_required,
            "contracter_runtime is required but does not exist"
        );
        eprintln!("contracter_runtime does not exist; runtime-role execution check skipped");
        return;
    }

    for relation in ["available_user_inventory", "available_warehouse_inventory"] {
        let can_select: bool =
            sqlx::query_scalar("SELECT has_table_privilege('contracter_runtime', $1, 'SELECT')")
                .bind(relation)
                .fetch_one(database.pool())
                .await
                .expect("inspect availability view privilege");
        assert!(can_select, "runtime role cannot select {relation}");
    }
    for relation in ["inventory_item_locks", "quote_candidate_reservations"] {
        let can_select: bool =
            sqlx::query_scalar("SELECT has_table_privilege('contracter_runtime', $1, 'SELECT')")
                .bind(relation)
                .fetch_one(database.pool())
                .await
                .expect("inspect guard table privilege");
        assert!(!can_select, "runtime role can read guard table {relation}");
    }

    let can_set_role: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user, 'contracter_runtime', 'SET')")
            .fetch_one(database.pool())
            .await
            .expect("inspect SET ROLE membership");
    if !can_set_role {
        assert!(
            !runtime_role_required,
            "migration user cannot SET ROLE contracter_runtime"
        );
        eprintln!("migration user cannot SET ROLE contracter_runtime; execution check skipped");
        return;
    }

    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SET LOCAL ROLE contracter_runtime")
        .await
        .expect("assume runtime role");
    let user_items = list_available_user_inventory(transaction.as_mut(), UserId::new(-1))
        .await
        .expect("runtime role lists user inventory");
    assert!(user_items.is_empty());
    list_available_warehouse_inventory(transaction.as_mut())
        .await
        .expect("runtime role lists warehouse inventory");

    // The owner's own reads must work under the same role, and for the
    // same reason: they report a `locked` flag derived from
    // `inventory_item_locks`, which this role cannot read. Only the
    // `owned_inventory` view (0017) runs with its owner's rights -- an
    // `EXISTS` subquery written inline here would be evaluated with this
    // role's privileges and fail. Asserted under the role rather than as
    // the migration superuser, because as the superuser it passes either
    // way; that is exactly how the inline version shipped once already.
    db::list_owned_inventory(transaction.as_mut(), UserId::new(-1), None, None, None, 10)
        .await
        .expect("runtime role lists the owner's inventory");
    db::find_owned_inventory_item(
        transaction.as_mut(),
        UserId::new(-1),
        PublicId::new(Uuid::new_v4()),
    )
    .await
    .expect("runtime role reads one owned item");
    transaction.rollback().await.expect("rollback role check");
}

/// The owner's own view of their inventory, which is not the same as the
/// *available* view the contract engine uses.
///
/// `available_user_inventory` hides a locked item, which is right for
/// "what can be spent" and wrong for "what do I own": an item silently
/// vanishing while it is reserved in a quote reads as theft. The owner's
/// listing shows it with a flag instead. Retired items stay hidden --
/// those are gone, not reserved.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn owner_inventory_shows_locked_items_as_locked_and_hides_retired_ones() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, unlocked_public_id, sku_id, user_id) = seed_inventory_item(&mut transaction).await;

    let (locked_id, locked_public_id) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(30_000_000, 8),
        Some(user_id),
        false,
        false,
    )
    .await;
    let (_, retired_public_id) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(40_000_000, 8),
        Some(user_id),
        false,
        true,
    )
    .await;
    let (quote_id, _) = seed_quote(&mut transaction, user_id).await;
    sqlx::query(
        "INSERT INTO inventory_item_locks (inventory_item_id, quote_id, expires_at) \
         VALUES ($1, $2, clock_timestamp() + interval '30 seconds')",
    )
    .bind(locked_id)
    .bind(quote_id)
    .execute(transaction.as_mut())
    .await
    .expect("lock inventory item");

    let items = db::list_owned_inventory(transaction.as_mut(), user_id, None, None, None, 200)
        .await
        .expect("list owned inventory");

    let locked = items
        .iter()
        .find(|row| row.public_id == locked_public_id)
        .expect("a reserved item is still owned, so the owner still sees it");
    assert!(
        locked.locked,
        "and it is shown as reserved rather than hidden"
    );

    let unlocked = items
        .iter()
        .find(|row| row.public_id == unlocked_public_id)
        .expect("the free item is listed");
    assert!(!unlocked.locked);

    assert!(
        !items.iter().any(|row| row.public_id == retired_public_id),
        "a retired item is gone, not reserved"
    );

    // Public context travels with the row, and no internal id does.
    assert_eq!(unlocked.wear_band_code, "inventory_test");
    assert_eq!(unlocked.rarity_code, "inventory_test");
    assert_eq!(unlocked.canonical_float, Decimal::new(12_345_678, 8));

    transaction.rollback().await.expect("rollback fixture");
}

/// Ownership is a SQL predicate, not a check the caller is trusted to
/// perform. Both the listing and the single-item read are scoped by owner
/// in the query itself, so there is no path where a caller who knows
/// another account's item UUID can read it.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn one_owner_can_never_read_another_owners_item() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, alice_item_public_id, sku_id, alice) = seed_inventory_item(&mut transaction).await;

    let bob: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') RETURNING id",
    )
    .bind(format!("inventory_bob_{}", Uuid::new_v4().simple()))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert second user");
    let (_, bob_item_public_id) = insert_inventory_item(
        &mut transaction,
        sku_id,
        Decimal::new(50_000_000, 8),
        Some(bob),
        false,
        false,
    )
    .await;

    let bobs_items = db::list_owned_inventory(transaction.as_mut(), bob, None, None, None, 200)
        .await
        .expect("list bob's inventory");
    assert!(
        bobs_items
            .iter()
            .all(|row| row.public_id != alice_item_public_id),
        "another account's item never appears in this account's listing"
    );

    // Knowing the UUID is not authorization: the owner is part of the
    // predicate, so the row simply does not exist for the wrong caller.
    assert!(
        db::find_owned_inventory_item(transaction.as_mut(), bob, alice_item_public_id)
            .await
            .expect("query")
            .is_none(),
        "reading another account's item by its public id must find nothing"
    );
    assert!(
        db::find_owned_inventory_item(transaction.as_mut(), alice, alice_item_public_id)
            .await
            .expect("query")
            .is_some(),
        "the real owner still reads their own item"
    );
    assert!(
        db::find_owned_inventory_item(transaction.as_mut(), bob, bob_item_public_id)
            .await
            .expect("query")
            .is_some()
    );

    // A UUID that belongs to nobody is the same answer as one that
    // belongs to someone else: absent.
    assert!(
        db::find_owned_inventory_item(transaction.as_mut(), bob, PublicId::new(Uuid::new_v4()))
            .await
            .expect("query")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

/// Paging walks the owner's inventory without skipping or repeating a
/// row, keyed on `(created_at, public_id)` so the order is total even
/// when several items were created in the same transaction.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn owner_inventory_pages_without_skipping_or_repeating() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, first_public_id, sku_id, user_id) = seed_inventory_item(&mut transaction).await;

    let mut expected = vec![first_public_id];
    for index in 1..6_i64 {
        let (_, public_id) = insert_inventory_item(
            &mut transaction,
            sku_id,
            Decimal::new(index * 1_000_000, 8),
            Some(user_id),
            false,
            false,
        )
        .await;
        expected.push(public_id);
    }
    expected.sort();

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = db::list_owned_inventory(transaction.as_mut(), user_id, None, None, cursor, 2)
            .await
            .expect("page");
        assert!(page.len() <= 2, "a page never exceeds its limit");
        let Some(last) = page.last() else { break };
        cursor = Some((last.created_at, last.public_id));
        seen.extend(page.into_iter().map(|row| row.public_id));
    }

    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        seen.len(),
        "no row is returned twice across pages"
    );
    assert_eq!(sorted, expected, "and no row is skipped");

    transaction.rollback().await.expect("rollback fixture");
}

/// Ordering is part of the contract, and sorting the result before
/// comparing it -- which the paging test above does, deliberately, to
/// check for gaps and duplicates -- cannot see a reversed `ORDER BY`.
/// This asserts the direction itself.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn owner_inventory_is_ordered_newest_first() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, oldest, sku_id, user_id) = seed_inventory_item(&mut transaction).await;

    let mut in_creation_order = vec![oldest];
    for index in 1..4_i64 {
        let (_, public_id) = insert_inventory_item(
            &mut transaction,
            sku_id,
            Decimal::new(index * 1_000_000, 8),
            Some(user_id),
            false,
            false,
        )
        .await;
        in_creation_order.push(public_id);
    }

    let items = db::list_owned_inventory(transaction.as_mut(), user_id, None, None, None, 200)
        .await
        .expect("list owned inventory");
    let returned: Vec<_> = items.iter().map(|row| row.public_id).collect();

    let mut newest_first = in_creation_order.clone();
    newest_first.reverse();
    assert_eq!(
        returned, newest_first,
        "the most recently acquired item comes first"
    );

    transaction.rollback().await.expect("rollback fixture");
}

/// The composite cursor exists for exactly one reason: `created_at` is
/// not unique. Every other test in this file lets the column default to
/// `clock_timestamp()`, which is volatile and hands each statement its
/// own instant, so none of them ever reproduces the tie the cursor was
/// built for. This one forces it.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn paging_is_total_when_every_item_shares_one_timestamp() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let (_, first, sku_id, user_id) = seed_inventory_item(&mut transaction).await;

    // One instant, six rows: without the public id in the sort key and
    // in the cursor comparison, a page boundary inside this group would
    // skip or repeat.
    let shared_instant: DateTime<Utc> =
        sqlx::query_scalar("SELECT clock_timestamp() + interval '1 second'")
            .fetch_one(transaction.as_mut())
            .await
            .expect("read one instant");

    let mut expected = vec![first];
    for index in 1..6_i64 {
        let (item_id, public_id) = insert_inventory_item(
            &mut transaction,
            sku_id,
            Decimal::new(index * 1_000_000, 8),
            Some(user_id),
            false,
            false,
        )
        .await;
        sqlx::query("UPDATE inventory_items SET created_at = $1 WHERE id = $2")
            .bind(shared_instant)
            .bind(item_id)
            .execute(transaction.as_mut())
            .await
            .expect("force an identical creation timestamp");
        expected.push(public_id);
    }
    // The seeded row keeps its own earlier timestamp; the other five now
    // tie exactly.
    let tied: Vec<_> = expected[1..].to_vec();

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = db::list_owned_inventory(transaction.as_mut(), user_id, None, None, cursor, 2)
            .await
            .expect("page");
        let Some(last) = page.last() else { break };
        cursor = Some((last.created_at, last.public_id));
        seen.extend(page.into_iter().map(|row| row.public_id));
    }

    let mut deduplicated = seen.clone();
    deduplicated.sort();
    deduplicated.dedup();
    assert_eq!(
        deduplicated.len(),
        seen.len(),
        "a tie must not cause a row to be returned twice"
    );

    let mut sorted_expected = expected.clone();
    sorted_expected.sort();
    assert_eq!(deduplicated, sorted_expected, "and none may be skipped");

    // Within the tie the order is by public id descending, which is what
    // makes the cursor's row comparison total.
    let mut tied_returned: Vec<_> = seen
        .iter()
        .copied()
        .filter(|id| tied.contains(id))
        .collect();
    let mut tied_expected = tied.clone();
    tied_expected.sort();
    tied_expected.reverse();
    assert_eq!(
        tied_returned, tied_expected,
        "equal timestamps are broken by public id, descending"
    );
    tied_returned.clear();

    transaction.rollback().await.expect("rollback fixture");
}
