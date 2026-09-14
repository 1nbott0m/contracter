use rust_decimal::Decimal;
use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{
    CriticalActionId, DatabaseError, PriceHaltId, PublicId, SkuId, ValuationSnapshotId,
    ValuationSnapshotItemId,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ValuationSnapshot {
    pub id: ValuationSnapshotId,
    pub public_id: PublicId,
    pub parent_snapshot_id: Option<ValuationSnapshotId>,
    pub formula_version: String,
    pub snapshot_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SnapshotValuation {
    pub id: ValuationSnapshotItemId,
    pub snapshot_id: ValuationSnapshotId,
    pub sku_id: SkuId,
    pub verified_price_microcredits: i64,
    pub source_code: String,
    pub window_days: i16,
    pub valid_sale_count: i32,
    pub evidence_cutoff_at: DateTime<Utc>,
    pub evidence_digest: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CurrentValuation {
    pub sku_id: SkuId,
    pub snapshot_id: ValuationSnapshotId,
    pub snapshot_item_id: ValuationSnapshotItemId,
    pub verified_price_microcredits: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PriceHalt {
    pub id: PriceHaltId,
    pub public_id: PublicId,
    pub sku_id: SkuId,
    pub reason_code: String,
    pub observed_ratio: Option<Decimal>,
    pub halted_at: DateTime<Utc>,
    pub lifted_at: Option<DateTime<Utc>>,
    pub lifted_by_critical_action_id: Option<CriticalActionId>,
}

pub async fn find_valuation_snapshot<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<ValuationSnapshot>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, parent_snapshot_id, formula_version, snapshot_at, \
                created_at, published_at \
         FROM valuation_snapshots WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_current_valuation<'e, E>(
    executor: E,
    sku_id: SkuId,
) -> Result<Option<CurrentValuation>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT sku_id, snapshot_id, snapshot_item_id, verified_price_microcredits, updated_at \
         FROM current_valuations WHERE sku_id = $1",
    )
    .bind(sku_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn list_snapshot_valuations<'e, E>(
    executor: E,
    snapshot_id: ValuationSnapshotId,
) -> Result<Vec<SnapshotValuation>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, snapshot_id, sku_id, verified_price_microcredits, source_code, \
                window_days, valid_sale_count, evidence_cutoff_at, evidence_digest \
         FROM valuation_snapshot_items \
         WHERE snapshot_id = $1 \
         ORDER BY sku_id",
    )
    .bind(snapshot_id)
    .fetch_all(executor)
    .await?)
}

pub async fn list_tradeable_current_valuations<'e, E>(
    executor: E,
) -> Result<Vec<CurrentValuation>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT valuation.sku_id, valuation.snapshot_id, valuation.snapshot_item_id, \
                valuation.verified_price_microcredits, valuation.updated_at \
         FROM current_valuations AS valuation \
         JOIN valuation_snapshot_items AS snapshot_item \
           ON snapshot_item.id = valuation.snapshot_item_id \
          AND snapshot_item.snapshot_id = valuation.snapshot_id \
          AND snapshot_item.sku_id = valuation.sku_id \
         JOIN price_sources AS source ON source.code = snapshot_item.source_code \
         JOIN skus AS sku ON sku.id = valuation.sku_id \
         JOIN catalog_items AS item ON item.id = sku.catalog_item_id \
         JOIN collections AS collection ON collection.id = item.collection_id \
         WHERE source.enabled \
           AND sku.enabled \
           AND item.enabled \
           AND collection.enabled \
           AND NOT EXISTS ( \
               SELECT 1 FROM price_halts AS halt \
               WHERE halt.sku_id = valuation.sku_id AND halt.lifted_at IS NULL \
           ) \
         ORDER BY valuation.sku_id",
    )
    .fetch_all(executor)
    .await?)
}

pub async fn list_active_price_halts<'e, E>(executor: E) -> Result<Vec<PriceHalt>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, sku_id, reason_code, observed_ratio, halted_at, \
                lifted_at, lifted_by_critical_action_id \
         FROM price_halts \
         WHERE lifted_at IS NULL \
         ORDER BY sku_id, id",
    )
    .fetch_all(executor)
    .await?)
}
