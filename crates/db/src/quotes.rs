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

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SeedEnvelope {
    pub allocation_public_id: PublicId,
    pub commitment_hash: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct QuoteProposalProjection {
    pub allocation_public_id: PublicId,
    pub commitment_hash: Vec<u8>,
    pub seed_nonce: Vec<u8>,
    pub seed_ciphertext: Vec<u8>,
    pub allocation_expires_at: DateTime<Utc>,
    pub inventory_item_id: InventoryItemId,
    pub inventory_item_public_id: PublicId,
    pub sku_id: SkuId,
    pub sku_public_id: PublicId,
    pub canonical_float: Decimal,
    pub locked_position_version: i64,
    pub catalog_item_id: crate::CatalogItemId,
    pub collection_id: crate::CollectionId,
    pub rarity_code: String,
    pub min_float: Decimal,
    pub max_float: Decimal,
    pub valuation_snapshot_id: ValuationSnapshotId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub verified_price_microcredits: i64,
    pub stock_policy_version_id: StockPolicyVersionId,
    pub risk_policy_version_id: RiskPolicyVersionId,
    pub signing_key_id: QuoteSigningKeyId,
    pub formula_version: String,
}

/// A server-computed and signed proposal, never deserialized from an HTTP body.
/// The authenticated owner and owner-bound allocation are separate from the
/// immutable quote document: PostgreSQL determines their internal IDs.
pub struct CreateTradeupQuote {
    pub user_id: UserId,
    pub allocation_public_id: PublicId,
    pub public_id: PublicId,
    pub valuation_snapshot_id: ValuationSnapshotId,
    pub stock_policy_version_id: StockPolicyVersionId,
    pub risk_policy_version_id: RiskPolicyVersionId,
    pub signing_key_id: QuoteSigningKeyId,
    pub formula_version: String,
    pub client_seed: Vec<u8>,
    pub nonce: i64,
    pub verified_input_value_microcredits: i64,
    pub expected_buyback_microcredits: i64,
    pub quote_total_microcredits: i64,
    pub adjustment_microcredits: i64,
    pub maximum_exposure_microcredits: i64,
    pub ordered_outcome_digest: [u8; 32],
    pub signature: [u8; 64],
    pub selected_outcome_position: i16,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub inputs: Vec<CreateQuoteInput>,
    pub outcomes: Vec<CreateQuoteOutcome>,
}

pub struct CreateQuoteInput {
    pub inventory_item_id: InventoryItemId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub locked_position_version: i64,
}

pub struct CreateQuoteOutcome {
    pub sku_id: SkuId,
    pub candidate_inventory_item_id: InventoryItemId,
    pub valuation_snapshot_item_id: ValuationSnapshotItemId,
    pub probability_numerator: i64,
    pub probability_denominator: i64,
    pub output_float: Decimal,
    pub buyback_microcredits: i64,
}

type JsonValue = sqlx::types::JsonValue;

fn json_object<const N: usize>(entries: [(&str, JsonValue); N]) -> JsonValue {
    JsonValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn lowercase_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 15)]));
    }
    encoded
}

impl CreateTradeupQuote {
    fn payloads(&self) -> (JsonValue, JsonValue, JsonValue) {
        let quote = json_object([
            ("public_id", self.public_id.get().to_string().into()),
            (
                "valuation_snapshot_id",
                self.valuation_snapshot_id.get().into(),
            ),
            (
                "stock_policy_version_id",
                self.stock_policy_version_id.get().into(),
            ),
            (
                "risk_policy_version_id",
                self.risk_policy_version_id.get().into(),
            ),
            ("signing_key_id", self.signing_key_id.get().into()),
            ("formula_version", self.formula_version.clone().into()),
            ("client_seed", lowercase_hex(&self.client_seed).into()),
            ("nonce", self.nonce.into()),
            (
                "verified_input_value_microcredits",
                self.verified_input_value_microcredits.into(),
            ),
            (
                "expected_buyback_microcredits",
                self.expected_buyback_microcredits.into(),
            ),
            (
                "quote_total_microcredits",
                self.quote_total_microcredits.into(),
            ),
            (
                "adjustment_microcredits",
                self.adjustment_microcredits.into(),
            ),
            (
                "maximum_exposure_microcredits",
                self.maximum_exposure_microcredits.into(),
            ),
            (
                "ordered_outcome_digest",
                lowercase_hex(&self.ordered_outcome_digest).into(),
            ),
            ("signature", lowercase_hex(&self.signature).into()),
            (
                "selected_outcome_position",
                self.selected_outcome_position.into(),
            ),
            ("created_at", self.created_at.to_rfc3339().into()),
            ("expires_at", self.expires_at.to_rfc3339().into()),
        ]);
        let inputs = JsonValue::Array(
            self.inputs
                .iter()
                .map(|input| {
                    json_object([
                        ("inventory_item_id", input.inventory_item_id.get().into()),
                        (
                            "valuation_snapshot_item_id",
                            input.valuation_snapshot_item_id.get().into(),
                        ),
                        (
                            "locked_position_version",
                            input.locked_position_version.into(),
                        ),
                    ])
                })
                .collect(),
        );
        let outcomes = JsonValue::Array(
            self.outcomes
                .iter()
                .map(|outcome| {
                    json_object([
                        ("sku_id", outcome.sku_id.get().into()),
                        (
                            "candidate_inventory_item_id",
                            outcome.candidate_inventory_item_id.get().into(),
                        ),
                        (
                            "valuation_snapshot_item_id",
                            outcome.valuation_snapshot_item_id.get().into(),
                        ),
                        (
                            "probability_numerator",
                            outcome.probability_numerator.into(),
                        ),
                        (
                            "probability_denominator",
                            outcome.probability_denominator.into(),
                        ),
                        // PostgreSQL casts the exact decimal text to numeric; never f64.
                        ("output_float", outcome.output_float.to_string().into()),
                        ("buyback_microcredits", outcome.buyback_microcredits.into()),
                    ])
                })
                .collect(),
        );
        (quote, inputs, outcomes)
    }
}

/// Persists an already-computed proposal atomically. PostgreSQL rechecks owner,
/// allocation expiry, positions, stock, and risk at the write boundary.
pub async fn create_tradeup_quote_for_user<'e, E>(
    executor: E,
    request: &CreateTradeupQuote,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    let (quote, inputs, outcomes) = request.payloads();
    Ok(
        sqlx::query_scalar("SELECT create_quote_for_user($1, $2, $3, $4, $5)")
            .bind(request.user_id)
            .bind(request.allocation_public_id)
            .bind(sqlx::types::Json(quote))
            .bind(sqlx::types::Json(inputs))
            .bind(sqlx::types::Json(outcomes))
            .fetch_one(executor)
            .await?,
    )
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

pub async fn read_seed_envelope_for_user<'e, E>(
    executor: E,
    user_id: UserId,
    allocation_public_id: PublicId,
) -> Result<Option<SeedEnvelope>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT allocation_public_id, commitment_hash, nonce, ciphertext \
         FROM read_seed_envelope_for_user($1, $2)",
    )
    .bind(user_id)
    .bind(allocation_public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn read_quote_proposal_projection<'e, E>(
    executor: E,
    user_id: UserId,
    allocation_public_id: PublicId,
    inventory_item_public_ids: &[PublicId],
) -> Result<Vec<QuoteProposalProjection>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    let ids: Vec<_> = inventory_item_public_ids
        .iter()
        .map(|id| id.get())
        .collect();
    Ok(sqlx::query_as(
        "SELECT allocation_public_id, commitment_hash, seed_nonce, seed_ciphertext,\
                allocation_expires_at, inventory_item_id, inventory_item_public_id, sku_id,\
                sku_public_id, canonical_float, locked_position_version, catalog_item_id,\
                collection_id, rarity_code, min_float, max_float, valuation_snapshot_id,\
                valuation_snapshot_item_id, verified_price_microcredits, stock_policy_version_id,\
                risk_policy_version_id, signing_key_id, formula_version\
         FROM read_quote_proposal_projection($1, $2, $3)",
    )
    .bind(user_id)
    .bind(allocation_public_id)
    .bind(&ids)
    .fetch_all(executor)
    .await?)
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

#[cfg(test)]
mod creation_tests {
    use super::*;

    #[test]
    fn atomic_writer_payload_preserves_exact_values_and_sql_contract() {
        let created_at = DateTime::from_timestamp(1_800_000_000, 123_456_000).unwrap();
        let request = CreateTradeupQuote {
            user_id: UserId::new(12),
            allocation_public_id: PublicId::new(uuid::Uuid::new_v4()),
            public_id: PublicId::new(uuid::Uuid::new_v4()),
            valuation_snapshot_id: ValuationSnapshotId::new(3),
            stock_policy_version_id: StockPolicyVersionId::new(4),
            risk_policy_version_id: RiskPolicyVersionId::new(5),
            signing_key_id: QuoteSigningKeyId::new(6),
            formula_version: "test/v1".to_owned(),
            client_seed: vec![0x00, 0xab, 0xff],
            nonce: 1,
            verified_input_value_microcredits: i64::MAX,
            expected_buyback_microcredits: 1234,
            quote_total_microcredits: 2000,
            adjustment_microcredits: -100,
            maximum_exposure_microcredits: 1334,
            ordered_outcome_digest: [0xab; 32],
            signature: [0xcd; 64],
            selected_outcome_position: 1,
            created_at,
            expires_at: DateTime::from_timestamp(1_800_000_015, 123_456_000).unwrap(),
            inputs: [19, 3, 25, 9]
                .into_iter()
                .map(|id| CreateQuoteInput {
                    inventory_item_id: InventoryItemId::new(id),
                    valuation_snapshot_item_id: ValuationSnapshotItemId::new(21),
                    locked_position_version: 42,
                })
                .collect(),
            outcomes: vec![CreateQuoteOutcome {
                sku_id: SkuId::new(11),
                candidate_inventory_item_id: InventoryItemId::new(12),
                valuation_snapshot_item_id: ValuationSnapshotItemId::new(23),
                probability_numerator: 100,
                probability_denominator: 100,
                output_float: Decimal::new(123456789012345678, 18),
                buyback_microcredits: 1234,
            }],
        };
        let (quote, inputs, outcomes) = request.payloads();
        assert_eq!(quote["client_seed"].as_str(), Some("00abff"));
        assert_eq!(quote["ordered_outcome_digest"], "ab".repeat(32));
        assert_eq!(quote["signature"], "cd".repeat(64));
        assert_eq!(
            quote["verified_input_value_microcredits"].as_i64(),
            Some(i64::MAX)
        );
        assert_eq!(quote["adjustment_microcredits"].as_i64(), Some(-100));
        assert_eq!(quote["selected_outcome_position"].as_i64(), Some(1));
        assert_eq!(quote["created_at"], request.created_at.to_rfc3339());
        assert_eq!(quote["expires_at"], request.expires_at.to_rfc3339());
        assert!(quote.get("user_id").is_none());
        assert!(quote.get("allocation_id").is_none());
        assert!(quote.get("commitment_id").is_none());
        let ids: Vec<_> = inputs
            .as_array()
            .unwrap()
            .iter()
            .map(|input| input["inventory_item_id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, vec![19, 3, 25, 9]);
        assert_eq!(inputs[0]["locked_position_version"].as_i64(), Some(42));
        assert_eq!(
            outcomes[0]["output_float"].as_str(),
            Some("0.123456789012345678")
        );
        assert_eq!(
            outcomes[0]["candidate_inventory_item_id"].as_i64(),
            Some(12)
        );
    }
}
