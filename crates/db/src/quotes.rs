use rust_decimal::Decimal;
use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{
    DatabaseError, InventoryItemId, PublicId, QuoteId, QuoteInputId, QuoteOutcomeId,
    QuoteSigningKeyId, RiskPolicyVersionId, SeedAllocationId, SeedCommitmentId, SkuId,
    StockPolicyVersionId, UserId, ValuationSnapshotId, ValuationSnapshotItemId,
};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TradeupQuote {
    pub id: QuoteId,
    pub public_id: PublicId,
    pub user_id: UserId,
    pub allocation_id: SeedAllocationId,
    pub commitment_id: SeedCommitmentId,
    pub valuation_snapshot_id: ValuationSnapshotId,
    pub stock_policy_version_id: StockPolicyVersionId,
    pub risk_policy_version_id: RiskPolicyVersionId,
    pub signing_key_id: QuoteSigningKeyId,
    pub status_code: String,
    pub formula_version: String,
    pub client_seed: Vec<u8>,
    pub nonce: i64,
    pub verified_input_value_microcredits: i64,
    pub expected_buyback_microcredits: i64,
    pub quote_total_microcredits: i64,
    pub adjustment_microcredits: i64,
    pub maximum_exposure_microcredits: i64,
    pub ordered_outcome_digest: Vec<u8>,
    pub signature: Vec<u8>,
    pub selected_outcome_position: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct QuoteInput {
    pub id: QuoteInputId,
    pub quote_id: QuoteId,
    pub position: i16,
    pub inventory_item_id: InventoryItemId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub locked_position_version: i64,
    pub inventory_item_public_id: PublicId,
    pub canonical_float: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct QuoteOutcome {
    pub id: QuoteOutcomeId,
    pub quote_id: QuoteId,
    pub position: i16,
    pub sku_id: SkuId,
    pub candidate_inventory_item_id: InventoryItemId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub probability_numerator: i64,
    pub probability_denominator: i64,
    pub output_float: Decimal,
    pub buyback_microcredits: i64,
    pub is_selected: bool,
    pub candidate_inventory_item_public_id: PublicId,
}

/// Server-produced payload for the first phase of a provably-fair quote.
/// All byte fields are validated again by PostgreSQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedAllocationRequest {
    pub user_id: UserId,
    pub commitment_hash: [u8; 32],
    pub encoding_version: String,
    pub nonce: [u8; 24],
    pub ciphertext: [u8; 48],
}

pub async fn allocate_seed_for_user<'e, E>(
    executor: E,
    request: &SeedAllocationRequest,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT allocate_seed_for_user($1, $2, $3, $4, $5)")
            .bind(request.user_id)
            .bind(request.commitment_hash.as_slice())
            .bind(&request.encoding_version)
            .bind(request.nonce.as_slice())
            .bind(request.ciphertext.as_slice())
            .fetch_one(executor)
            .await?,
    )
}

pub async fn find_tradeup_quote<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<TradeupQuote>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, user_id, allocation_id, commitment_id, valuation_snapshot_id, \
                stock_policy_version_id, risk_policy_version_id, signing_key_id, status_code, \
                formula_version, client_seed, nonce, verified_input_value_microcredits, \
                expected_buyback_microcredits, quote_total_microcredits, adjustment_microcredits, \
                maximum_exposure_microcredits, ordered_outcome_digest, signature, \
                selected_outcome_position, created_at, expires_at \
         FROM tradeup_quotes WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_active_quote_for_user<'e, E>(
    executor: E,
    user_id: UserId,
) -> Result<Option<TradeupQuote>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, user_id, allocation_id, commitment_id, valuation_snapshot_id, \
                stock_policy_version_id, risk_policy_version_id, signing_key_id, status_code, \
                formula_version, client_seed, nonce, verified_input_value_microcredits, \
                expected_buyback_microcredits, quote_total_microcredits, adjustment_microcredits, \
                maximum_exposure_microcredits, ordered_outcome_digest, signature, \
                selected_outcome_position, created_at, expires_at \
         FROM tradeup_quotes \
         WHERE user_id = $1 AND status_code = 'active' \
           AND expires_at > clock_timestamp()",
    )
    .bind(user_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn list_quote_inputs<'e, E>(
    executor: E,
    quote_id: QuoteId,
) -> Result<Vec<QuoteInput>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT input.id, input.quote_id, input.position, input.inventory_item_id, \
                input.valuation_snapshot_item_id, input.locked_position_version, \
                item.public_id AS inventory_item_public_id, item.canonical_float \
         FROM quote_inputs AS input \
         JOIN inventory_items AS item ON item.id = input.inventory_item_id \
         WHERE input.quote_id = $1 ORDER BY input.position, input.id",
    )
    .bind(quote_id)
    .fetch_all(executor)
    .await?)
}

pub async fn list_quote_outcomes<'e, E>(
    executor: E,
    quote_id: QuoteId,
) -> Result<Vec<QuoteOutcome>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT outcome.id, outcome.quote_id, outcome.position, outcome.sku_id, \
                outcome.candidate_inventory_item_id, outcome.valuation_snapshot_item_id, \
                outcome.probability_numerator, outcome.probability_denominator, \
                outcome.output_float, outcome.buyback_microcredits, outcome.is_selected, \
                item.public_id AS candidate_inventory_item_public_id \
         FROM quote_outcomes AS outcome \
         JOIN inventory_items AS item ON item.id = outcome.candidate_inventory_item_id \
         WHERE outcome.quote_id = $1 ORDER BY outcome.position, outcome.id",
    )
    .bind(quote_id)
    .fetch_all(executor)
    .await?)
}
