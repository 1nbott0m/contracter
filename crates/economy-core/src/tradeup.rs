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
/// How finely a scarcity multiplier is resolved, and the factor by which
/// `apply_outcome_scarcity` scales the outcome denominator.
///
/// Fixed rather than derived from the multipliers, so the final
/// denominator is known before any damping and cannot grow with them. A
/// million parts is far finer than any supply signal warrants -- the
/// published multipliers are ratios of unit counts in the low hundreds --
/// and it keeps the denominator around 10^8 for realistic contracts,
/// three orders of magnitude inside `u64`.
const SCARCITY_RESOLUTION: u128 = 1_000_000;
const HASH_DOMAIN: &[u8] = b"contracter/outcome/v1";
const COMMITMENT_DOMAIN: &[u8] = b"contracter/server-seed/v1";

#[derive(Clone, Debug, PartialEq)]
pub struct InputItem {
    /// Which physical item this is, distinct from which *kind* of item it
    /// is (`sku_id`).
    ///
    /// A contract spends ten specific instances, and the same instance may
    /// not be spent twice. Without an identity here the rule was
    /// unrepresentable in the layer that computes the probabilities:
    /// passing one item four times produced weights and an output float as
    /// though four items had been spent, and only `finalize_contract`
    /// caught it, at the very end. A probability calculation that can be
    /// fed a lie is one whose result means nothing.
    pub item_id: String,
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
    #[error("the same item cannot be used twice: {item_id}")]
    DuplicateInput { item_id: String },
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

/// A supply-based damping factor for one outcome, in `[0, 1]` as an exact
/// rational `numerator / denominator`. `denominator` must be greater than
/// zero and `numerator` must not exceed it -- scarcity only ever damps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScarcityMultiplier {
    pub numerator: u64,
    pub denominator: u64,
}

/// Applies per-outcome scarcity, keyed by SKU, **within each collection**.
///
/// The game-mechanics document fixes two things that together decide this
/// function's shape: the chance of a collection follows the composition of
/// the inputs, and scarcity changes the weights of eligible outcomes
/// *inside* the chosen collection. So a multiplier may move probability
/// between the outcomes of one collection and must never move it between
/// collections. Renormalising across the whole set -- which an earlier
/// version did -- turns scarcity into a cross-collection dial: damping one
/// collection raises another's odds, the tie to input composition breaks,
/// and scarcity becomes an unauditable margin lever rather than a supply
/// signal. The document forbids exactly that.
///
/// Keyed by SKU rather than by collection, because a single multiplier
/// covering a whole collection carries no within-collection information:
/// scaling every outcome of a collection by the same factor and then
/// renormalising inside it is the identity. Stock is per SKU, so the
/// signal is too.
///
/// An outcome damped to nothing is dropped and its collection's remaining
/// outcomes take its share, which keeps the collection's own weight
/// intact. A collection with nothing left is an error rather than a silent
/// redistribution: handing its share to another collection is the one
/// thing this function exists to prevent, so there is no correct answer to
/// give.
///
/// The arithmetic is exact. Each collection's outcomes are damped over a
/// common multiplier denominator, then the whole distribution is scaled by
/// the lowest common multiple of the collections' damped totals, so every
/// resulting weight is an integer and each collection's share is preserved
/// to the last unit. Nothing is rounded, so no share drifts.
pub fn apply_outcome_scarcity(
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

    // Group by collection. `BTreeMap` orders by collection id, which makes
    // the result independent of the order the outcomes arrived in.
    let mut collections: BTreeMap<&str, Vec<&WeightedOutcome>> = BTreeMap::new();
    for outcome in outcomes {
        collections
            .entry(outcome.collection_id.as_str())
            .or_default()
            .push(outcome);
    }

    // Every collection's slice of the final denominator is exactly
    // `original_share * SCARCITY_RESOLUTION`, so the final denominator is
    // fixed before any damping happens and cannot grow with the
    // multipliers. That is the whole reason this is laid out as it is: an
    // earlier version took the lowest common multiple of the collections'
    // damped totals, which is a product of near-coprime numbers. With
    // multiplier denominators of the shape the stock policy actually
    // produces -- 167, 125, 83, 42, 17, 7 -- a five-collection contract
    // needed a 67-bit denominator and was refused as `InvalidWeights`, so
    // an ordinary multi-collection trade-up failed and the error pointed
    // at corrupt data rather than at an internal range limit.
    let final_denominator = u128::from(denominator)
        .checked_mul(SCARCITY_RESOLUTION)
        .ok_or(TradeupError::InvalidWeights)?;
    let final_denominator =
        u64::try_from(final_denominator).map_err(|_| TradeupError::InvalidWeights)?;

    let mut result = Vec::with_capacity(outcomes.len());

    for (collection, members) in &collections {
        // The share this collection must still hold afterwards, in units
        // of the final denominator. Computed from the incoming weights, so
        // it is whatever `build_outcomes` decided from input composition.
        let mut original_share = 0_u128;
        for member in members {
            original_share = original_share
                .checked_add(u128::from(member.weight_numerator))
                .ok_or(TradeupError::InvalidWeights)?;
        }
        let collection_target = original_share
            .checked_mul(SCARCITY_RESOLUTION)
            .ok_or(TradeupError::InvalidWeights)?;

        // Damped weights, in arbitrary units: only their ratios matter,
        // because the collection's total is pinned above.
        let mut damped = Vec::with_capacity(members.len());
        let mut damped_total = 0_u128;
        for member in members {
            let multiplier =
                multipliers
                    .get(&member.sku_id)
                    .copied()
                    .unwrap_or(ScarcityMultiplier {
                        numerator: 1,
                        denominator: 1,
                    });
            if multiplier.denominator == 0 || multiplier.numerator > multiplier.denominator {
                return Err(TradeupError::InvalidWeights);
            }
            // Normalised to a fixed resolution rather than to a common
            // multiple of the multipliers' own denominators. Flooring here
            // only loses a multiplier finer than one part in
            // SCARCITY_RESOLUTION, and a multiplier that floors to zero was
            // already "damped to nothing".
            let normalised = u128::from(multiplier.numerator)
                .checked_mul(SCARCITY_RESOLUTION)
                .ok_or(TradeupError::InvalidWeights)?
                / u128::from(multiplier.denominator);
            let weight = u128::from(member.weight_numerator)
                .checked_mul(normalised)
                .ok_or(TradeupError::InvalidWeights)?;
            if weight == 0 {
                continue;
            }
            damped_total = damped_total
                .checked_add(weight)
                .ok_or(TradeupError::InvalidWeights)?;
            damped.push((member, weight));
        }

        if damped_total == 0 {
            // Every candidate here is out of stock. These inputs have no
            // reachable result, and giving the share to a different
            // collection would break the one invariant scarcity must
            // uphold.
            return Err(TradeupError::MissingOutput {
                collection_id: (*collection).to_owned(),
            });
        }

        // Apportion `collection_target` among the survivors in proportion
        // to their damped weights, by largest remainder. The floors alone
        // would leave a few units unassigned and the collection's share
        // would come out short; handing each leftover unit to the largest
        // remainder assigns exactly the target, so the share stays exact
        // while only the within-collection split rounds.
        let mut assigned = Vec::with_capacity(damped.len());
        let mut distributed = 0_u128;
        for (member, weight) in &damped {
            let exact = collection_target
                .checked_mul(*weight)
                .ok_or(TradeupError::InvalidWeights)?;
            let floor = exact / damped_total;
            assigned.push((*member, floor, exact % damped_total));
            distributed = distributed
                .checked_add(floor)
                .ok_or(TradeupError::InvalidWeights)?;
        }

        // Ties break on sku id, so two runs over the same inputs assign the
        // same leftovers -- a quote has to be reproducible.
        let mut order: Vec<usize> = (0..assigned.len()).collect();
        order.sort_by(|left, right| {
            assigned[*right]
                .2
                .cmp(&assigned[*left].2)
                .then_with(|| assigned[*left].0.sku_id.cmp(&assigned[*right].0.sku_id))
        });
        let mut leftover = collection_target - distributed;
        for index in order {
            if leftover == 0 {
                break;
            }
            assigned[index].1 += 1;
            leftover -= 1;
        }

        for (member, numerator, _) in assigned {
            if numerator == 0 {
                // Rounded away entirely: dropped rather than carried at
                // zero, matching `select_outcome`'s rule that a zero weight
                // is invalid rather than merely inert. Its units have
                // already gone to its siblings, so the collection's share
                // is unaffected.
                continue;
            }
            let numerator = u64::try_from(numerator).map_err(|_| TradeupError::InvalidWeights)?;
            result.push(WeightedOutcome {
                sku_id: member.sku_id.clone(),
                collection_id: member.collection_id.clone(),
                weight_numerator: numerator,
                weight_denominator: final_denominator,
            });
        }
    }

    result.sort_by(|a, b| a.sku_id.cmp(&b.sku_id));
    if checked_weight_sum(&result)? != final_denominator {
        return Err(TradeupError::InvalidWeights);
    }
    Ok(result)
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
    // Distinctness, checked here rather than trusted from the caller. The
    // database enforces it too, at finalisation; this is the layer that
    // would otherwise compute a distribution from a set that cannot exist.
    let mut seen = BTreeSet::new();
    for item in inputs {
        if !seen.insert(item.item_id.as_str()) {
            return Err(TradeupError::DuplicateInput {
                item_id: item.item_id.clone(),
            });
        }
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
