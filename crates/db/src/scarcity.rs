use sqlx::{
    Executor, PgConnection, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{
    CollectionId, CollectionScarcitySnapshotId, CollectionScarcitySnapshotItemId, DatabaseError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct CollectionScarcity {
    pub collection_id: CollectionId,
    pub snapshot_id: CollectionScarcitySnapshotId,
    pub snapshot_item_id: CollectionScarcitySnapshotItemId,
    pub weight_multiplier_numerator: i64,
    pub weight_multiplier_denominator: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct CollectionScarcitySnapshotItem {
    pub id: CollectionScarcitySnapshotItemId,
    pub snapshot_id: CollectionScarcitySnapshotId,
    pub collection_id: CollectionId,
    pub available_units_total: i32,
    pub target_units_total: i32,
    pub weight_multiplier_numerator: i64,
    pub weight_multiplier_denominator: i64,
}

/// The current published scarcity multiplier for a collection. `None` means
/// no snapshot has ever covered it (callers should treat that as `1/1`, the
/// same default `crates/economy-core` applies to a collection missing from
/// its multiplier map).
pub async fn find_current_collection_scarcity<'e, E>(
    executor: E,
    collection_id: CollectionId,
) -> Result<Option<CollectionScarcity>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT collection_id, snapshot_id, snapshot_item_id, weight_multiplier_numerator, \
                weight_multiplier_denominator, updated_at \
         FROM current_collection_scarcity WHERE collection_id = $1",
    )
    .bind(collection_id)
    .fetch_optional(executor)
    .await?)
}

/// Full scarcity-snapshot history for one collection, oldest first.
pub async fn list_collection_scarcity_history<'e, E>(
    executor: E,
    collection_id: CollectionId,
) -> Result<Vec<CollectionScarcitySnapshotItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, snapshot_id, collection_id, available_units_total, target_units_total, \
                weight_multiplier_numerator, weight_multiplier_denominator \
         FROM collection_scarcity_snapshot_items \
         WHERE collection_id = $1 \
         ORDER BY snapshot_id",
    )
    .bind(collection_id)
    .fetch_all(executor)
    .await?)
}

/// Computes and publishes a fresh collection-scarcity snapshot from current
/// `warehouse_stock` against the active `stock_policy_bands`, returning the
/// new snapshot's id. Only collections with a positive target under the
/// active stock policy get a row (see the module-level formula note in the
/// migration: no coverage means no damping, not zero weight).
pub async fn publish_collection_scarcity_snapshot(
    connection: &mut PgConnection,
    formula_version: &str,
) -> Result<CollectionScarcitySnapshotId, DatabaseError> {
    let snapshot_id: CollectionScarcitySnapshotId = sqlx::query_scalar(
        "INSERT INTO collection_scarcity_snapshots (formula_version, snapshot_at) \
         VALUES ($1, clock_timestamp()) RETURNING id",
    )
    .bind(formula_version)
    .fetch_one(&mut *connection)
    .await?;

    sqlx::query(
        "INSERT INTO collection_scarcity_snapshot_items ( \
             snapshot_id, collection_id, available_units_total, target_units_total, \
             weight_multiplier_numerator, weight_multiplier_denominator \
         ) \
         WITH collection_sku_targets AS ( \
             SELECT catalog_items.collection_id, skus.id AS sku_id, band.target_units \
               FROM skus \
               JOIN catalog_items ON catalog_items.id = skus.catalog_item_id \
               JOIN stock_policy_bands AS band \
                 ON band.rarity_code = catalog_items.rarity_code \
                AND band.stock_policy_version_id = ( \
                    SELECT id FROM stock_policy_versions \
                     WHERE activated_at IS NOT NULL AND retired_at IS NULL \
                     ORDER BY activated_at DESC, id DESC LIMIT 1) \
              WHERE catalog_items.enabled AND skus.enabled \
         ), \
         aggregated AS ( \
             SELECT collection_sku_targets.collection_id, \
                    COALESCE(SUM(warehouse_stock.available_units), 0)::integer \
                        AS available_units_total, \
                    SUM(collection_sku_targets.target_units)::integer AS target_units_total \
               FROM collection_sku_targets \
               LEFT JOIN warehouse_stock \
                 ON warehouse_stock.sku_id = collection_sku_targets.sku_id \
              GROUP BY collection_sku_targets.collection_id \
         ) \
         SELECT $1, \
                aggregated.collection_id, \
                aggregated.available_units_total, \
                aggregated.target_units_total, \
                LEAST(aggregated.available_units_total, aggregated.target_units_total), \
                aggregated.target_units_total \
           FROM aggregated \
          WHERE aggregated.target_units_total > 0",
    )
    .bind(snapshot_id)
    .execute(&mut *connection)
    .await?;

    sqlx::query("SELECT publish_collection_scarcity_snapshot($1)")
        .bind(snapshot_id)
        .execute(&mut *connection)
        .await?;

    Ok(snapshot_id)
}
