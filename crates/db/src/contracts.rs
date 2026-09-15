use rust_decimal::Decimal;
use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{
    ContractId, ContractInputId, ContractOutcomeId, DatabaseError, InventoryItemId,
    LedgerTransactionId, PublicId, QuoteId, QuoteOutcomeId, SkuId, StockPolicyVersionId, UserId,
    ValuationSnapshotId, ValuationSnapshotItemId,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Contract {
    pub id: ContractId,
    pub public_id: PublicId,
    pub quote_id: QuoteId,
    pub user_id: UserId,
    pub status_code: String,
    pub valuation_snapshot_id: ValuationSnapshotId,
    pub stock_policy_version_id: StockPolicyVersionId,
    pub formula_version: String,
    pub ledger_transaction_id: Option<LedgerTransactionId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ContractInput {
    pub id: ContractInputId,
    pub contract_id: ContractId,
    pub position: i16,
    pub inventory_item_id: InventoryItemId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub input_float: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ContractOutcome {
    pub id: ContractOutcomeId,
    pub contract_id: ContractId,
    pub quote_outcome_id: QuoteOutcomeId,
    pub inventory_item_id: InventoryItemId,
    pub sku_id: SkuId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub output_float: Decimal,
    pub probability_numerator: i64,
    pub probability_denominator: i64,
    pub buyback_microcredits: i64,
}

pub async fn find_contract_by_public_id<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<Contract>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, quote_id, user_id, status_code, valuation_snapshot_id, \
                stock_policy_version_id, formula_version, ledger_transaction_id, created_at \
         FROM contracts WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_contract_by_quote_id<'e, E>(
    executor: E,
    quote_id: QuoteId,
) -> Result<Option<Contract>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, quote_id, user_id, status_code, valuation_snapshot_id, \
                stock_policy_version_id, formula_version, ledger_transaction_id, created_at \
         FROM contracts WHERE quote_id = $1",
    )
    .bind(quote_id)
    .fetch_optional(executor)
    .await?)
}

/// Contract inputs, stably ordered by `position` then `id`.
pub async fn list_contract_inputs<'e, E>(
    executor: E,
    contract_id: ContractId,
) -> Result<Vec<ContractInput>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, contract_id, position, inventory_item_id, valuation_snapshot_item_id, \
                input_float \
         FROM contract_inputs WHERE contract_id = $1 ORDER BY position, id",
    )
    .bind(contract_id)
    .fetch_all(executor)
    .await?)
}

/// The single outcome of a contract (`contract_outcomes.contract_id` is
/// unique), if one has been recorded.
pub async fn find_contract_outcome<'e, E>(
    executor: E,
    contract_id: ContractId,
) -> Result<Option<ContractOutcome>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, contract_id, quote_outcome_id, inventory_item_id, sku_id, \
                valuation_snapshot_item_id, output_float, probability_numerator, \
                probability_denominator, buyback_microcredits \
         FROM contract_outcomes WHERE contract_id = $1",
    )
    .bind(contract_id)
    .fetch_optional(executor)
    .await?)
}
