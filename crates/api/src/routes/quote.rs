use application::quote;
use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
pub struct SeedAllocationResponse {
    pub allocation_id: uuid::Uuid,
    pub commitment: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateQuoteRequest {
    pub allocation_id: Uuid,
    pub item_ids: Vec<Uuid>,
    /// UTF-8 text, interpreted as bytes without normalization or trimming.
    pub client_seed: String,
}

impl From<CreateQuoteRequest> for quote::CreateQuoteRequest {
    fn from(request: CreateQuoteRequest) -> Self {
        Self {
            allocation_id: db::PublicId::new(request.allocation_id),
            item_ids: request
                .item_ids
                .into_iter()
                .map(db::PublicId::new)
                .collect(),
            client_seed: request.client_seed.into_bytes(),
        }
    }
}

/// `POST /api/v1/me/quotes` accepts only public request data. Creation remains
/// unavailable until server-side proposal construction is fully implemented.
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Json(request): Json<CreateQuoteRequest>,
) -> Result<Json<QuoteResponse>, ApiError> {
    let request = quote::CreateQuoteRequest::from(request);
    request.validate()?;
    let protector = state
        .seed_protector()
        .ok_or_else(|| ApiError::service_unavailable("Quote creation is unavailable"))?;
    let signer = state
        .quote_signer()
        .ok_or_else(|| ApiError::service_unavailable("Quote creation is unavailable"))?;
    let quote = quote::create(
        state.database(),
        caller.user_id,
        &request,
        protector.as_ref(),
        signer.as_ref(),
    )
    .await?;
    Ok(Json(quote_response(quote)))
}

/// `POST /api/v1/me/quote-allocations` fixes a server-seed commitment before
/// any client seed or inventory item IDs are submitted.
pub async fn allocate(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<Json<SeedAllocationResponse>, ApiError> {
    let protector = state
        .seed_protector()
        .ok_or_else(|| ApiError::service_unavailable("Quote creation is unavailable"))?;
    let allocation =
        quote::allocate_seed(state.database(), caller.user_id, protector.as_ref()).await?;
    Ok(Json(SeedAllocationResponse {
        allocation_id: allocation.public_id.get(),
        commitment: allocation.commitment,
    }))
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptQuoteRequest {
    pub idempotency_key: Uuid,
}

#[derive(Serialize)]
pub struct AcceptQuoteResponse {
    pub contract_id: Uuid,
}

pub async fn active(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<Json<QuoteResponse>, ApiError> {
    let quote = quote::find_active(state.database(), caller.user_id).await?;
    Ok(Json(quote_response(quote)))
}

fn quote_response(quote: quote::ActiveQuote) -> QuoteResponse {
    QuoteResponse {
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
    }
}

/// `POST /api/v1/me/quote/{quote_id}/accept` -- finalizes the caller's
/// already-signed quote. The database validates all mutable conditions in one
/// transaction; this route deliberately accepts no client-provided item IDs.
pub async fn accept(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Path(quote_id): Path<Uuid>,
    Json(request): Json<AcceptQuoteRequest>,
) -> Result<Json<AcceptQuoteResponse>, ApiError> {
    let contract = quote::accept(
        state.database(),
        caller.user_id,
        db::PublicId::new(quote_id),
        request.idempotency_key,
    )
    .await?;
    Ok(Json(AcceptQuoteResponse {
        contract_id: contract.contract_public_id.get(),
    }))
}

#[cfg(test)]
mod creation_tests {
    use super::*;
    use application::{
        auth::{AuthConfig, AuthenticatedUser},
        quote_signing::EnvironmentQuoteSigner,
        seed_protection::EnvironmentSeedProtector,
    };
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        response::IntoResponse,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tower::ServiceExt;

    fn body() -> Value {
        json!({
            "allocation_id": Uuid::new_v4(),
            "item_ids": (0..4).map(|_| Uuid::new_v4()).collect::<Vec<_>>(),
            "client_seed": "my seed",
        })
    }

    fn state() -> AppState {
        AppState::new(
            db::Database::connect_lazy(
                &db::DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable").unwrap(),
            ),
            AuthConfig::default(),
        )
    }

    fn caller() -> CurrentUser {
        CurrentUser(AuthenticatedUser {
            user_id: db::UserId::new(1),
            user_public_id: db::PublicId::new(Uuid::new_v4()),
            session_public_id: db::PublicId::new(Uuid::new_v4()),
        })
    }

    #[test]
    fn client_cannot_supply_economics_identity_or_secret_fields() {
        for field in [
            "price",
            "quote_total_microcredits",
            "probabilities",
            "outcomes",
            "signature",
            "user_id",
            "server_seed",
            "nonce",
            "commitment",
            "ciphertext",
            "key",
        ] {
            let mut request = body();
            request[field] = json!("untrusted");
            assert!(
                serde_json::from_value::<CreateQuoteRequest>(request).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn request_keeps_input_order_and_utf8_seed_bytes() {
        let mut body = body();
        body["client_seed"] = json!("  мой seed  ");
        let parsed: CreateQuoteRequest = serde_json::from_value(body.clone()).unwrap();
        let request = quote::CreateQuoteRequest::from(parsed);
        assert_eq!(request.client_seed, "  мой seed  ".as_bytes());
        assert_eq!(
            request.item_ids[0].get().to_string(),
            body["item_ids"][0].as_str().unwrap()
        );
        assert!(request.validate().is_ok());
    }

    #[tokio::test]
    async fn creation_requires_authentication_before_processing_body() {
        let router = crate::router::build_router(state(), &crate::router::RouterConfig::default());
        let response = router
            .oneshot(
                Request::post("/api/v1/me/quotes")
                    .header("content-type", "application/json")
                    .body(Body::from(body().to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["cache-control"], "no-store");
    }

    #[tokio::test]
    async fn invalid_inputs_are_rejected_before_any_database_write() {
        for invalid in [json!([]), json!(vec![Uuid::nil(); 4])] {
            let mut body = body();
            body["item_ids"] = invalid;
            let response = create(
                State(state()),
                caller(),
                Json(serde_json::from_value(body).unwrap()),
            )
            .await
            .err()
            .unwrap()
            .into_response();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn absent_or_configured_crypto_cannot_bypass_missing_proposal_builder() {
        const TEST_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for (protector, signer) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut state = state();
            if protector {
                state = state.with_seed_protector(Arc::new(
                    EnvironmentSeedProtector::from_base64url(TEST_KEY).unwrap(),
                ));
            }
            if signer {
                state = state.with_quote_signer(Arc::new(
                    EnvironmentQuoteSigner::from_base64url(TEST_KEY).unwrap(),
                ));
            }
            let response = create(
                State(state),
                caller(),
                Json(serde_json::from_value(body()).unwrap()),
            )
            .await
            .err()
            .unwrap()
            .into_response();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            let response: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(response["error"]["code"], "SERVICE_UNAVAILABLE");
            for forbidden in ["server_seed", "nonce", "ciphertext", "signature", TEST_KEY] {
                assert!(!String::from_utf8_lossy(&bytes).contains(forbidden));
            }
        }
    }
}
