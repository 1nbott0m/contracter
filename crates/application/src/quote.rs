use db::{Database, DatabaseError, PublicId, UserId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuoteError {
    #[error("there is no active quote")]
    NotFound,
    #[error("quote data is inconsistent")]
    Inconsistent,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

pub struct ActiveQuote {
    pub public_id: PublicId,
    pub formula_version: String,
    pub verified_input_value_microcredits: i64,
    pub expected_buyback_microcredits: i64,
    pub quote_total_microcredits: i64,
    pub expires_at: db::chrono::DateTime<db::chrono::Utc>,
    pub inputs: Vec<QuoteInput>,
    pub outcomes: Vec<QuoteOutcome>,
}

pub struct QuoteInput {
    pub position: i16,
    pub item_public_id: PublicId,
    pub canonical_float: rust_decimal::Decimal,
}

pub struct QuoteOutcome {
    pub position: i16,
    pub item_public_id: PublicId,
    pub output_float: rust_decimal::Decimal,
    pub probability_numerator: i64,
    pub probability_denominator: i64,
    pub buyback_microcredits: i64,
}

pub async fn find_active(database: &Database, owner: UserId) -> Result<ActiveQuote, QuoteError> {
    let quote = db::find_active_quote_for_user(database.pool(), owner)
        .await?
        .ok_or(QuoteError::NotFound)?;
    let mut inputs = Vec::new();
    for input in db::list_quote_inputs(database.pool(), quote.id).await? {
        inputs.push(QuoteInput {
            position: input.position,
            item_public_id: input.inventory_item_public_id,
            canonical_float: input.canonical_float,
        });
    }
    let mut outcomes = Vec::new();
    for outcome in db::list_quote_outcomes(database.pool(), quote.id).await? {
        outcomes.push(QuoteOutcome {
            position: outcome.position,
            item_public_id: outcome.candidate_inventory_item_public_id,
            output_float: outcome.output_float,
            probability_numerator: outcome.probability_numerator,
            probability_denominator: outcome.probability_denominator,
            buyback_microcredits: outcome.buyback_microcredits,
        });
    }
    Ok(ActiveQuote {
        public_id: quote.public_id,
        formula_version: quote.formula_version,
        verified_input_value_microcredits: quote.verified_input_value_microcredits,
        expected_buyback_microcredits: quote.expected_buyback_microcredits,
        quote_total_microcredits: quote.quote_total_microcredits,
        expires_at: quote.expires_at,
        inputs,
        outcomes,
    })
}
