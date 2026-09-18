use rust_decimal::{Decimal, RoundingStrategy};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// A contract takes four to ten inputs, inclusive.
///
/// The count is not a constant in the probability maths that follow: the
/// collection weights and the output float both divide by the number of
/// inputs actually submitted. Dividing by a fixed ten instead would make
/// every shorter contract's weights fail to sum to one, and would bias
/// every output float toward Factory New.
const INPUT_RANGE: std::ops::RangeInclusive<usize> = 4..=10;
const COVERT_RARITY: u8 = 5;
const MIN_WEIGHT_DENOMINATOR: u64 = 100;
const HASH_DOMAIN: &[u8] = b"contracter/outcome/v1";
const COMMITMENT_DOMAIN: &[u8] = b"contracter/server-seed/v1";

#[derive(Clone, Debug, PartialEq)]
pub struct InputItem {
    pub sku_id: String,
    pub collection_id: String,
    pub rarity: u8,
    pub float: Decimal,
    pub min_float: Decimal,
    pub max_float: Decimal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputItem {
    pub sku_id: String,
    pub collection_id: String,
    pub rarity: u8,
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedOutcome {
    pub sku_id: String,
    pub collection_id: String,
    pub weight_numerator: u64,
    pub weight_denominator: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub sku_id: String,
    pub digest: [u8; 32],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TradeupError {
    #[error("between {} and {} inputs are required, got {actual}", INPUT_RANGE.start(), INPUT_RANGE.end())]
    InputCount { actual: usize },
    #[error("input rarities must match")]
    MixedInputRarities,
    #[error("Covert inputs are not eligible")]
    CovertInput,
    #[error("rarity {rarity} cannot have a next tier")]
    InvalidRarity { rarity: u8 },
    #[error("invalid float range for {sku_id}")]
    InvalidFloatRange { sku_id: String },
    #[error("float is outside its catalog range for {sku_id}")]
    FloatOutOfRange { sku_id: String },
    #[error("collection {collection_id} has no next-rarity output")]
    MissingOutput { collection_id: String },
    #[error("candidate output has the wrong rarity: {sku_id}")]
    WrongOutputRarity { sku_id: String },
    #[error("outcome weights are invalid")]
    InvalidWeights,
}

pub fn build_outcomes(
    inputs: &[InputItem],
    outputs: &[OutputItem],
) -> Result<Vec<WeightedOutcome>, TradeupError> {
    validate_inputs(inputs)?;
    let input_rarity = inputs[0].rarity;
    let output_rarity = input_rarity
        .checked_add(1)
        .ok_or(TradeupError::InvalidRarity {
            rarity: input_rarity,
        })?;
    let mut counts = BTreeMap::<&str, u64>::new();
    for item in inputs {
        *counts.entry(item.collection_id.as_str()).or_default() += 1;
    }

    let represented = counts.keys().copied().collect::<BTreeSet<_>>();
    let mut grouped = BTreeMap::<&str, Vec<&OutputItem>>::new();
    for output in outputs {
        if !represented.contains(output.collection_id.as_str()) {
            continue;
        }
        if output.rarity != output_rarity {
            return Err(TradeupError::WrongOutputRarity {
                sku_id: output.sku_id.clone(),
            });
        }
        // Zero stock excludes the candidate; it does not doom the
        // contract. The remaining candidates are renormalised by the
        // weight maths below, which divides by how many survived in each
        // collection. Refusing outright -- the previous behaviour --
        // meant one out-of-stock skin made every contract touching its
        // collection impossible.
        //
        // A collection left with nothing in stock is still refused, but
        // as `MissingOutput` below, which is the honest description: the
        // inputs have no reachable next-rarity result.
        if !output.available {
            continue;
        }
        grouped
            .entry(output.collection_id.as_str())
            .or_default()
            .push(output);
    }

    // The divisor is the submitted count, not a constant. Because the
    // per-collection input counts sum to exactly this, the per-outcome
    // weights still sum to `common` for any accepted count.
    let input_total = inputs.len() as u64;
    let mut common = 1_u64;
    for collection in counts.keys() {
        let count = grouped
            .get(collection)
            .ok_or_else(|| TradeupError::MissingOutput {
                collection_id: (*collection).to_owned(),
            })?
            .len() as u64;
        let scaled = input_total
            .checked_mul(count)
            .ok_or(TradeupError::InvalidWeights)?;
        common = checked_lcm(common, scaled).ok_or(TradeupError::InvalidWeights)?;
    }
    if common < MIN_WEIGHT_DENOMINATOR {
        common *= MIN_WEIGHT_DENOMINATOR.div_ceil(common);
    }

    let mut result = Vec::new();
    for (collection, input_count) in counts {
        let collection_outputs = &grouped[collection];
        let each = input_count
            .checked_mul(common)
            .and_then(|v| {
                input_total
                    .checked_mul(collection_outputs.len() as u64)
                    .and_then(|divisor| v.checked_div(divisor))
            })
            .ok_or(TradeupError::InvalidWeights)?;
        for output in collection_outputs {
            result.push(WeightedOutcome {
                sku_id: output.sku_id.clone(),
                collection_id: output.collection_id.clone(),
                weight_numerator: each,
                weight_denominator: common,
            });
        }
    }
    result.sort_by(|a, b| a.sku_id.cmp(&b.sku_id));
    if checked_weight_sum(&result)? != common {
        return Err(TradeupError::InvalidWeights);
    }
    Ok(result)
}

/// A collection's supply-based damping factor, in `[0, 1]` as an exact
/// rational `numerator / denominator`. `denominator` must be greater than
/// zero and `numerator` must not exceed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScarcityMultiplier {
    pub numerator: u64,
    pub denominator: u64,
}

/// Applies a per-collection scarcity multiplier to already-built outcome
/// weights. A collection missing from `multipliers` is treated as `1/1`
/// (no damping). An outcome whose damped weight rounds down to zero is
/// dropped rather than carried through with a zero weight, matching
/// [`select_outcome`]'s existing rule that a zero-weight entry is invalid
/// rather than merely inert. The result's denominator is simply the sum of
/// the surviving damped numerators, which already satisfies
/// [`select_outcome`]'s "numerators sum to the shared denominator"
/// invariant without rescaling back to the original total.
///
/// Each outcome's damped weight floors to an integer independently
/// (`weight_numerator * multiplier.numerator / multiplier.denominator`).
/// For small `weight_numerator` values -- which `build_outcomes` routinely
/// produces -- two different multipliers can floor to the same integer for
/// every outcome and so produce a bit-identical result: e.g. two outcomes
/// each with `weight_numerator = 3` damped by 50/100 and by 40/60 both
/// floor to `(1, 1)`. A caller publishing a materially different scarcity
/// multiplier is not guaranteed any observable effect on low-weight
/// outcomes; see
/// `draining_stock_cannot_increase_a_collections_own_weight_but_may_have_no_effect_at_low_weight`.
pub fn apply_collection_scarcity(
    outcomes: &[WeightedOutcome],
    multipliers: &BTreeMap<String, ScarcityMultiplier>,
) -> Result<Vec<WeightedOutcome>, TradeupError> {
    let denominator = outcomes
        .first()
        .map(|outcome| outcome.weight_denominator)
        .ok_or(TradeupError::InvalidWeights)?;
    if outcomes
        .iter()
        .any(|outcome| outcome.weight_denominator != denominator)
    {
        return Err(TradeupError::InvalidWeights);
    }

    let mut damped = Vec::with_capacity(outcomes.len());
    for outcome in outcomes {
        let multiplier =
            multipliers
                .get(&outcome.collection_id)
                .copied()
                .unwrap_or(ScarcityMultiplier {
                    numerator: 1,
                    denominator: 1,
                });
        if multiplier.denominator == 0 || multiplier.numerator > multiplier.denominator {
            return Err(TradeupError::InvalidWeights);
        }
        let scaled = (outcome.weight_numerator as u128)
            .checked_mul(multiplier.numerator as u128)
            .ok_or(TradeupError::InvalidWeights)?;
        let damped_numerator = scaled
            .checked_div(multiplier.denominator as u128)
            .ok_or(TradeupError::InvalidWeights)?;
        if damped_numerator == 0 {
            continue;
        }
        let damped_numerator =
            u64::try_from(damped_numerator).map_err(|_| TradeupError::InvalidWeights)?;
        damped.push((outcome, damped_numerator));
    }

    let total = damped
        .iter()
        .try_fold(0_u64, |sum, (_, numerator)| sum.checked_add(*numerator))
        .ok_or(TradeupError::InvalidWeights)?;
    if total == 0 {
        return Err(TradeupError::InvalidWeights);
    }

    Ok(damped
        .into_iter()
        .map(|(outcome, numerator)| WeightedOutcome {
            sku_id: outcome.sku_id.clone(),
            collection_id: outcome.collection_id.clone(),
            weight_numerator: numerator,
            weight_denominator: total,
        })
        .collect())
}

pub fn calculate_output_float(
    inputs: &[InputItem],
    output_min: Decimal,
    output_max: Decimal,
) -> Result<Decimal, TradeupError> {
    validate_inputs(inputs)?;
    if output_min >= output_max {
        return Err(TradeupError::InvalidFloatRange {
            sku_id: "output".to_owned(),
        });
    }
    let sum = inputs.iter().try_fold(Decimal::ZERO, |total, item| {
        let normalized = (item.float - item.min_float) / (item.max_float - item.min_float);
        total
            .checked_add(normalized)
            .ok_or(TradeupError::InvalidWeights)
    })?;
    let average = sum / Decimal::from(inputs.len());
    let value = output_min + average * (output_max - output_min);
    Ok(value.round_dp_with_strategy(8, RoundingStrategy::MidpointNearestEven))
}

pub fn server_seed_commitment(server_seed: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    append_field(&mut hasher, COMMITMENT_DOMAIN);
    append_field(&mut hasher, server_seed);
    hasher.finalize().into()
}

pub fn select_outcome(
    outcomes: &[WeightedOutcome],
    server_seed: &[u8; 32],
    client_seed: &[u8],
    nonce: u64,
) -> Result<Selection, TradeupError> {
    let denominator = outcomes
        .first()
        .map(|outcome| outcome.weight_denominator)
        .ok_or(TradeupError::InvalidWeights)?;
    if outcomes
        .iter()
        .any(|outcome| outcome.weight_denominator != denominator || outcome.weight_numerator == 0)
        || checked_weight_sum(outcomes)? != denominator
    {
        return Err(TradeupError::InvalidWeights);
    }

    let mut ordered = outcomes.to_vec();
    ordered.sort_by(|a, b| a.sku_id.cmp(&b.sku_id));
    let threshold = (denominator as u128).wrapping_neg() % denominator as u128;
    let mut counter = 0_u64;
    loop {
        let digest = selection_digest(&ordered, server_seed, client_seed, nonce, counter);
        let value = u128::from_be_bytes(digest[..16].try_into().expect("fixed digest length"));
        counter = counter.checked_add(1).ok_or(TradeupError::InvalidWeights)?;
        if value < threshold {
            continue;
        }
        let mut ticket = (value % denominator as u128) as u64;
        for outcome in &ordered {
            if ticket < outcome.weight_numerator {
                return Ok(Selection {
                    sku_id: outcome.sku_id.clone(),
                    digest,
                });
            }
            ticket -= outcome.weight_numerator;
        }
        return Err(TradeupError::InvalidWeights);
    }
}

fn validate_inputs(inputs: &[InputItem]) -> Result<(), TradeupError> {
    if !INPUT_RANGE.contains(&inputs.len()) {
        return Err(TradeupError::InputCount {
            actual: inputs.len(),
        });
    }
    let rarity = inputs[0].rarity;
    if inputs.iter().any(|item| item.rarity != rarity) {
        return Err(TradeupError::MixedInputRarities);
    }
    if rarity == COVERT_RARITY {
        return Err(TradeupError::CovertInput);
    }
    for item in inputs {
        if item.min_float >= item.max_float {
            return Err(TradeupError::InvalidFloatRange {
                sku_id: item.sku_id.clone(),
            });
        }
        if item.float < item.min_float || item.float > item.max_float {
            return Err(TradeupError::FloatOutOfRange {
                sku_id: item.sku_id.clone(),
            });
        }
    }
    Ok(())
}

fn selection_digest(
    outcomes: &[WeightedOutcome],
    server_seed: &[u8; 32],
    client_seed: &[u8],
    nonce: u64,
    counter: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    append_field(&mut hasher, HASH_DOMAIN);
    append_field(&mut hasher, server_seed);
    append_field(&mut hasher, client_seed);
    append_field(&mut hasher, &nonce.to_be_bytes());
    append_field(&mut hasher, &counter.to_be_bytes());
    for outcome in outcomes {
        append_field(&mut hasher, outcome.sku_id.as_bytes());
        append_field(&mut hasher, &outcome.weight_numerator.to_be_bytes());
        append_field(&mut hasher, &outcome.weight_denominator.to_be_bytes());
    }
    hasher.finalize().into()
}

fn append_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn checked_lcm(a: u64, b: u64) -> Option<u64> {
    (a / gcd(a, b)).checked_mul(b)
}

fn checked_weight_sum(outcomes: &[WeightedOutcome]) -> Result<u64, TradeupError> {
    outcomes.iter().try_fold(0_u64, |sum, outcome| {
        sum.checked_add(outcome.weight_numerator)
            .ok_or(TradeupError::InvalidWeights)
    })
}
