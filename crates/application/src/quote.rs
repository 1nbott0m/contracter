use crate::seed_protection::{SeedProtectionError, SeedProtector};
use db::{Database, DatabaseError, PublicId, SeedAllocationRequest, UserId};
use economy_core::tradeup::server_seed_commitment;
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuoteError {
    #[error("a quote requires 4 to 10 distinct item IDs and a client seed of 1 to 1024 bytes")]
    InvalidRequest,
    #[error("quote creation is unavailable")]
    CreationUnavailable,
    #[error("there is no active quote")]
    NotFound,
    #[error("quote data is inconsistent")]
    Inconsistent,
    #[error("quote cannot be accepted")]
    NotAcceptable,
    #[error("an active quote allocation already exists")]
    ActiveAllocation,
    #[error("seed protection failed")]
    SeedProtection(#[from] SeedProtectionError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Decrypts the owner-bound envelope returned by the database reader.  The
/// lengths and commitment are checked here as a second boundary: malformed
/// database data must never reach the deterministic trade-up algorithm.
pub fn decrypt_server_seed(
    envelope: &db::SeedEnvelope,
    protector: &(impl SeedProtector + ?Sized),
) -> Result<[u8; 32], QuoteError> {
    let commitment: [u8; 32] = envelope
        .commitment_hash
        .as_slice()
        .try_into()
        .map_err(|_| QuoteError::Inconsistent)?;
    let nonce: [u8; 24] = envelope
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| QuoteError::Inconsistent)?;
    let ciphertext: [u8; 48] = envelope
        .ciphertext
        .as_slice()
        .try_into()
        .map_err(|_| QuoteError::Inconsistent)?;
    let seed = protector.decrypt(
        &crate::seed_protection::ProtectedSeed {
            nonce,
            ciphertext: ciphertext.to_vec(),
        },
        &commitment,
    )?;
    if server_seed_commitment(&seed) != commitment {
        return Err(QuoteError::Inconsistent);
    }
    Ok(seed)
}

/// Canonical digest input for a quote proposal.  Length-prefixing removes
/// ambiguity between variable-length UUID/decimal/text fields and keeps the
/// signed representation independent of JSON key ordering.
pub fn canonical_quote_digest(
    allocation_id: PublicId,
    client_seed: &[u8],
    ordered_outcome_digest: &[u8; 32],
    formula_version: &str,
) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(128 + client_seed.len() + formula_version.len());
    bytes.extend_from_slice(b"contracter/quote/v1\0");
    for field in [
        allocation_id.get().as_bytes().as_slice(),
        client_seed,
        ordered_outcome_digest,
        formula_version.as_bytes(),
    ] {
        bytes.extend_from_slice(&(field.len() as u32).to_be_bytes());
        bytes.extend_from_slice(field);
    }
    Sha256::digest(bytes).into()
}

/// The entire client-controlled quote input. Prices, candidates, weights,
/// signatures and seeds from the server are deliberately absent.
pub struct CreateQuoteRequest {
    pub allocation_id: PublicId,
    pub item_ids: Vec<PublicId>,
    pub client_seed: Vec<u8>,
}

impl CreateQuoteRequest {
    pub fn validate(&self) -> Result<(), QuoteError> {
        let distinct = self
            .item_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>();
        if !(economy_core::tradeup::MIN_INPUT_COUNT..=economy_core::tradeup::MAX_INPUT_COUNT)
            .contains(&self.item_ids.len())
            || distinct.len() != self.item_ids.len()
            || !(1..=1024).contains(&self.client_seed.len())
        {
            return Err(QuoteError::InvalidRequest);
        }
        Ok(())
    }
}

/// Resolves the owner-bound proposal context and decrypts the server seed
/// before proposal construction. The final writer is still fail-closed until
/// candidate output selection and pricing are available in the projection.
pub async fn create(
    _database: &Database,
    _owner: UserId,
    request: &CreateQuoteRequest,
    _protector: &(impl SeedProtector + ?Sized),
    _signer: &(impl crate::quote_signing::QuoteSigner + ?Sized),
) -> Result<ActiveQuote, QuoteError> {
    request.validate()?;
    let projection = db::read_quote_proposal_projection(
        _database.pool(),
        _owner,
        request.allocation_id,
        &request.item_ids,
    )
    .await?;
    if projection.len() != request.item_ids.len()
        || projection
            .iter()
            .zip(&request.item_ids)
            .any(|(row, requested)| row.inventory_item_public_id != *requested)
    {
        return Err(QuoteError::Inconsistent);
    }
    let first = projection.first().ok_or(QuoteError::Inconsistent)?;
    let envelope = db::SeedEnvelope {
        allocation_public_id: first.allocation_public_id,
        commitment_hash: first.commitment_hash.clone(),
        nonce: first.seed_nonce.clone(),
        ciphertext: first.seed_ciphertext.clone(),
    };
    let _server_seed = decrypt_server_seed(&envelope, _protector)?;
    // The projection deliberately contains no candidate outputs. Do not
    // synthesize them from client data or publish an unsigned proposal.
    let _ = _signer;
    Err(QuoteError::CreationUnavailable)
}

pub struct AcceptedQuote {
    pub contract_public_id: PublicId,
}

pub struct SeedAllocation {
    pub public_id: PublicId,
    pub commitment: [u8; 32],
}

pub async fn allocate_seed(
    database: &Database,
    owner: UserId,
    protector: &(impl SeedProtector + ?Sized),
) -> Result<SeedAllocation, QuoteError> {
    let mut server_seed = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut server_seed);
    let commitment = server_seed_commitment(&server_seed);
    let protected = protector.encrypt(&server_seed, &commitment)?;
    let ciphertext: [u8; 48] = protected
        .ciphertext
        .try_into()
        .map_err(|_| QuoteError::SeedProtection(SeedProtectionError::Encryption))?;
    let public_id = db::allocate_seed_for_user(
        database.pool(),
        &SeedAllocationRequest {
            user_id: owner,
            commitment_hash: commitment,
            encoding_version: "contracter/server-seed/v1".to_owned(),
            nonce: protected.nonce,
            ciphertext,
        },
    )
    .await
    .map_err(|error| match error.database_code().as_deref() {
        Some("23505") => QuoteError::ActiveAllocation,
        _ => QuoteError::Database(error),
    })?;
    Ok(SeedAllocation {
        public_id,
        commitment,
    })
}

/// Accepts a quote through the database's owner-bound finalization function.
/// The client supplies only an idempotency key: the locked input set is
/// recovered from the immutable quote, never trusted from an HTTP body.
pub async fn accept(
    database: &Database,
    owner: UserId,
    quote_public_id: PublicId,
    idempotency_key: uuid::Uuid,
) -> Result<AcceptedQuote, QuoteError> {
    let quote = db::find_tradeup_quote(database.pool(), quote_public_id)
        .await?
        .ok_or(QuoteError::NotFound)?;
    let input_ids = db::list_quote_inputs(database.pool(), quote.id)
        .await?
        .into_iter()
        .map(|input| input.inventory_item_id)
        .collect::<Vec<_>>();

    let contract_id = db::finalize_contract_for_user(
        database.pool(),
        owner,
        quote.id,
        &input_ids,
        idempotency_key,
    )
    .await
    .map_err(map_acceptance_error)?;
    let contract = db::find_contract_by_quote_id(database.pool(), quote.id)
        .await?
        .filter(|contract| contract.id == contract_id)
        .ok_or(QuoteError::Inconsistent)?;

    Ok(AcceptedQuote {
        contract_public_id: contract.public_id,
    })
}

fn map_acceptance_error(error: DatabaseError) -> QuoteError {
    match error.database_code().as_deref() {
        Some("42501") => QuoteError::NotFound,
        Some("23514") | Some("23505") => QuoteError::NotAcceptable,
        _ => QuoteError::Database(error),
    }
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

#[cfg(test)]
mod creation_tests {
    use super::*;

    fn request(count: usize) -> CreateQuoteRequest {
        CreateQuoteRequest {
            allocation_id: PublicId::new(uuid::Uuid::new_v4()),
            item_ids: (0..count)
                .map(|_| PublicId::new(uuid::Uuid::new_v4()))
                .collect(),
            client_seed: b"client-chosen-seed".to_vec(),
        }
    }

    #[test]
    fn input_count_must_be_four_through_ten() {
        for count in 0..=12 {
            assert_eq!(request(count).validate().is_ok(), (4..=10).contains(&count));
        }
    }

    #[test]
    fn duplicate_ids_do_not_count_as_distinct_inputs() {
        let mut request = request(4);
        request.item_ids[3] = request.item_ids[0];
        assert!(matches!(
            request.validate(),
            Err(QuoteError::InvalidRequest)
        ));
    }

    #[test]
    fn client_seed_limit_counts_bytes() {
        let mut request = request(4);
        for (length, valid) in [(0, false), (1, true), (1024, true), (1025, false)] {
            request.client_seed = vec![42; length];
            assert_eq!(request.validate().is_ok(), valid);
        }
    }
}
