use sqlx::{
    Executor, Postgres,
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
///
/// A single call to the `SECURITY DEFINER` SQL function: the computation,
/// insertion, and publish all happen inside its body, so this is atomic by
/// construction (one statement) and needs only `EXECUTE`, never direct
/// `INSERT` on the underlying tables — matching every other privileged
/// writer in this schema (`post_credit_adjustment`, `finalize_contract`).
pub async fn publish_collection_scarcity_snapshot<'e, E>(
    executor: E,
    formula_version: &str,
) -> Result<CollectionScarcitySnapshotId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT publish_collection_scarcity_snapshot($1)")
            .bind(formula_version)
            .fetch_one(executor)
            .await?,
    )
}
