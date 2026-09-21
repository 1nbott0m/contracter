use application::quote;
use axum::{Json, extract::State};
use serde::Serialize;

use crate::{error::ApiError, extract::CurrentUser, state::AppState};

#[derive(Serialize)]
pub struct QuoteResponse {
    pub quote_id: uuid::Uuid,
    pub formula_version: String,
    pub input_value_microcredits: i64,
    pub expected_buyback_microcredits: i64,
    pub total_microcredits: i64,
    pub expires_at: String,
    pub inputs: Vec<QuoteInputResponse>,
    pub outcomes: Vec<QuoteOutcomeResponse>,
}
#[derive(Serialize)]
pub struct QuoteInputResponse {
    pub position: i16,
    pub item_id: uuid::Uuid,
    pub canonical_float: String,
}
#[derive(Serialize)]
pub struct QuoteOutcomeResponse {
    pub position: i16,
    pub item_id: uuid::Uuid,
    pub output_float: String,
    pub probability_numerator: i64,
    pub probability_denominator: i64,
    pub buyback_microcredits: i64,
}

pub async fn active(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<Json<QuoteResponse>, ApiError> {
    let quote = quote::find_active(state.database(), caller.user_id).await?;
    Ok(Json(QuoteResponse {
        quote_id: quote.public_id.get(),
        formula_version: quote.formula_version,
        input_value_microcredits: quote.verified_input_value_microcredits,
        expected_buyback_microcredits: quote.expected_buyback_microcredits,
        total_microcredits: quote.quote_total_microcredits,
        expires_at: quote.expires_at.to_rfc3339(),
        inputs: quote
            .inputs
            .into_iter()
            .map(|input| QuoteInputResponse {
                position: input.position,
                item_id: input.item_public_id.get(),
                canonical_float: input.canonical_float.to_string(),
            })
            .collect(),
        outcomes: quote
            .outcomes
            .into_iter()
            .map(|outcome| QuoteOutcomeResponse {
                position: outcome.position,
                item_id: outcome.item_public_id.get(),
                output_float: outcome.output_float.to_string(),
                probability_numerator: outcome.probability_numerator,
                probability_denominator: outcome.probability_denominator,
                buyback_microcredits: outcome.buyback_microcredits,
            })
            .collect(),
    }))
}
