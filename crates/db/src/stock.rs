use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{
    DatabaseError, PublicId, RiskPolicyVersionId, SkuId, StockPolicyBandId, StockPolicyVersionId,
    ValuationSnapshotId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct WarehouseStock {
    pub sku_id: SkuId,
    pub available_units: i32,
    pub reserved_units: i32,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StockPolicyVersion {
    pub id: StockPolicyVersionId,
    pub public_id: PublicId,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub retired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StockPolicyBand {
    pub id: StockPolicyBandId,
    pub stock_policy_version_id: StockPolicyVersionId,
    pub rarity_code: String,
    pub minimum_units: i32,
    pub target_units: i32,
    pub maximum_units: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct RiskState {
    pub valuation_snapshot_id: Option<ValuationSnapshotId>,
    pub risk_policy_version_id: Option<RiskPolicyVersionId>,
    pub liquid_reserve_microcredits: i64,
    pub stressed_liability_microcredits: i64,
    pub outstanding_quote_exposure_microcredits: i64,
    pub version: i64,
    pub updated_at: DateTime<Utc>,
}

pub async fn find_warehouse_stock<'e, E>(
    executor: E,
    sku_id: SkuId,
) -> Result<Option<WarehouseStock>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT sku_id, available_units, reserved_units, version, updated_at \
         FROM warehouse_stock WHERE sku_id = $1",
    )
    .bind(sku_id)
    .fetch_optional(executor)
    .await?)
}

/// The single stock policy version that is activated and not yet retired, if
/// any. Ties (which should not occur) resolve to the most recently activated
/// version.
pub async fn find_active_stock_policy_version<'e, E>(
    executor: E,
) -> Result<Option<StockPolicyVersion>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, version, created_at, activated_at, retired_at \
         FROM stock_policy_versions \
         WHERE activated_at IS NOT NULL AND retired_at IS NULL \
         ORDER BY activated_at DESC, id DESC \
         LIMIT 1",
    )
    .fetch_optional(executor)
    .await?)
}

/// Stock bands of the single active stock policy version, stably ordered by
/// `id`. Empty when no stock policy version is currently active.
pub async fn find_active_stock_policy_bands<'e, E>(
    executor: E,
) -> Result<Vec<StockPolicyBand>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT band.id, band.stock_policy_version_id, band.rarity_code, \
                band.minimum_units, band.target_units, band.maximum_units \
         FROM stock_policy_bands AS band \
         WHERE band.stock_policy_version_id = ( \
             SELECT version.id FROM stock_policy_versions AS version \
              WHERE version.activated_at IS NOT NULL AND version.retired_at IS NULL \
              ORDER BY version.activated_at DESC, version.id DESC \
              LIMIT 1 \
         ) \
         ORDER BY band.id",
    )
    .fetch_all(executor)
    .await?)
}

/// The current singleton risk-state projection. The row always exists once
/// migrations have applied, so this reads exactly one row.
pub async fn find_risk_state<'e, E>(executor: E) -> Result<RiskState, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT valuation_snapshot_id, risk_policy_version_id, \
                liquid_reserve_microcredits, stressed_liability_microcredits, \
                outstanding_quote_exposure_microcredits, version, updated_at \
         FROM risk_state WHERE singleton",
    )
    .fetch_one(executor)
    .await?)
}
