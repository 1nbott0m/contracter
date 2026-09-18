use rust_decimal::Decimal;
use thiserror::Error;

/// The sell-back spread: converting an item to credits pays 85% of its
/// verified market value.
///
/// This is not the house edge and must not be confused with it. It
/// applies when a player *leaves* the item economy; the house edge below
/// applies when a player runs a contract. A player who contracts and
/// never sells pays the edge and never the spread.
const BUYBACK_PERCENT: i64 = 85;
const PERCENT_DENOMINATOR: i64 = 100;
const MIN_SALES: usize = 20;
/// Basis points, so 10_000 is one whole.
const BPS_DENOMINATOR: i64 = 10_000;

/// The house edge on a contract, in basis points: 8%.
///
/// The margin is taken at contract time, by charging more than the
/// outcome distribution is worth, rather than by damping the outcome
/// probabilities. Keeping it out of the weights is what lets collection
/// probability stay exactly proportional to input composition and lets
/// scarcity remain a supply signal rather than a hidden margin dial --
/// the two must stay separable, or neither can be audited.
pub const HOUSE_EDGE_BPS: i64 = 800;

/// What a contract is expected to return, as a fraction of what it costs:
/// 92%. The complement of [`HOUSE_EDGE_BPS`], derived rather than written
/// down twice so the two cannot drift apart.
pub const TARGET_EV_BPS: i64 = BPS_DENOMINATOR - HOUSE_EDGE_BPS;

/// The least an item may be worth and still be tradeable: 20 credits.
///
/// One credit is 1,000,000 microcredits, so this is 20_000_000. The floor
/// exists so that rounding, spreads and fees stay small relative to the
/// amounts they act on: on a one-microcredit item every one of them would
/// dominate the price.
pub const MINIMUM_ITEM_VALUE_MICROCREDITS: i64 = 20_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PricedOutcome {
    pub verified_price_microcredits: i64,
    pub weight_numerator: u64,
    pub weight_denominator: u64,
}

impl PricedOutcome {
    pub const fn new(
        verified_price_microcredits: i64,
        weight_numerator: u64,
        weight_denominator: u64,
    ) -> Self {
        Self {
            verified_price_microcredits,
            weight_numerator,
            weight_denominator,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuotePrice {
    pub expected_buyback_microcredits: i64,
    pub quote_total_microcredits: i64,
    pub adjustment_microcredits: i64,
    pub fee_microcredits: i64,
    pub rebate_microcredits: i64,
    pub effective_spread: Decimal,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PricingError {
    #[error("at least twenty completed sales are required, got {actual}")]
    InsufficientSales { actual: usize },
    #[error("amount cannot be negative")]
    NegativeAmount,
    #[error("probabilities must share a denominator and sum to one")]
    InvalidProbabilities,
    #[error("checked arithmetic overflow")]
    Overflow,
    #[error(
        "an item must be worth at least {MINIMUM_ITEM_VALUE_MICROCREDITS} microcredits, got {value_microcredits}"
    )]
    ItemBelowMinimumValue { value_microcredits: i64 },
}

/// Rejects an item too cheap to trade.
///
/// A negative value is reported as [`PricingError::NegativeAmount`]
/// rather than as "too cheap": it is not a price at all, and collapsing
/// the two would let a data fault be read as an ordinary business
/// refusal.
pub const fn validate_item_value(value_microcredits: i64) -> Result<(), PricingError> {
    if value_microcredits < 0 {
        return Err(PricingError::NegativeAmount);
    }
    if value_microcredits < MINIMUM_ITEM_VALUE_MICROCREDITS {
        return Err(PricingError::ItemBelowMinimumValue { value_microcredits });
    }
    Ok(())
}

pub fn trimmed_mean_microcredits(sales: &[i64]) -> Result<i64, PricingError> {
    if sales.len() < MIN_SALES {
        return Err(PricingError::InsufficientSales {
            actual: sales.len(),
        });
    }
    if sales.iter().any(|value| *value < 0) {
        return Err(PricingError::NegativeAmount);
    }
    let mut ordered = sales.to_vec();
    ordered.sort_unstable();
    let trim = ordered.len() / 10;
    let kept = &ordered[trim..ordered.len() - trim];
    let sum = kept.iter().try_fold(0_i128, |total, value| {
        total
            .checked_add(i128::from(*value))
            .ok_or(PricingError::Overflow)
    })?;
    round_ratio_half_even(sum, kept.len() as i128)
}

pub fn buyback_microcredits(price_microcredits: i64) -> Result<i64, PricingError> {
    if price_microcredits < 0 {
        return Err(PricingError::NegativeAmount);
    }
    let whole = (price_microcredits / PERCENT_DENOMINATOR)
        .checked_mul(BUYBACK_PERCENT)
        .ok_or(PricingError::Overflow)?;
    let remainder = (price_microcredits % PERCENT_DENOMINATOR)
        .checked_mul(BUYBACK_PERCENT)
        .ok_or(PricingError::Overflow)?
        / PERCENT_DENOMINATOR;
    whole.checked_add(remainder).ok_or(PricingError::Overflow)
}

pub fn quote_adjustment_microcredits(
    verified_input_value_microcredits: i64,
    outcomes: &[PricedOutcome],
) -> Result<QuotePrice, PricingError> {
    if verified_input_value_microcredits < 0 {
        return Err(PricingError::NegativeAmount);
    }
    if outcomes.is_empty() {
        return Err(PricingError::InvalidProbabilities);
    }
    let common_denominator = outcomes.iter().try_fold(1_i128, |common, outcome| {
        if outcome.weight_denominator == 0 || outcome.weight_numerator == 0 {
            return Err(PricingError::InvalidProbabilities);
        }
        checked_lcm_i128(common, i128::from(outcome.weight_denominator))
            .ok_or(PricingError::Overflow)
    })?;
    let weight_sum = outcomes.iter().try_fold(0_i128, |sum, outcome| {
        let scaled = i128::from(outcome.weight_numerator)
            .checked_mul(common_denominator / i128::from(outcome.weight_denominator))
            .ok_or(PricingError::Overflow)?;
        sum.checked_add(scaled).ok_or(PricingError::Overflow)
    })?;
    if weight_sum != common_denominator {
        return Err(PricingError::InvalidProbabilities);
    }

    let market_numerator = outcomes.iter().try_fold(0_i128, |sum, outcome| {
        // The floor is enforced here, on the path a quote actually takes,
        // rather than merely being defined: an outcome cheaper than the
        // minimum tradeable value must not be priced into a contract at
        // all. Defining the constant without consulting it is how a rule
        // ends up documented but absent.
        validate_item_value(outcome.verified_price_microcredits)?;
        let scaled_weight = i128::from(outcome.weight_numerator)
            .checked_mul(common_denominator / i128::from(outcome.weight_denominator))
            .ok_or(PricingError::Overflow)?;
        let weighted = i128::from(outcome.verified_price_microcredits)
            .checked_mul(scaled_weight)
            .ok_or(PricingError::Overflow)?;
        sum.checked_add(weighted).ok_or(PricingError::Overflow)
    })?;
    let expected_buyback_numerator = market_numerator
        .checked_mul(i128::from(BUYBACK_PERCENT))
        .ok_or(PricingError::Overflow)?;
    let expected_buyback_denominator = common_denominator
        .checked_mul(i128::from(PERCENT_DENOMINATOR))
        .ok_or(PricingError::Overflow)?;
    // Floored, matching `buyback_microcredits` exactly.
    //
    // This used to round half-even, which meant the figure quoted before
    // acceptance could exceed the figure settlement pays by a microcredit
    // -- two rounding rules for one quantity, and the discrepancy always
    // fell against the player. A quote has to promise what it will pay.
    let expected = expected_buyback_numerator
        .checked_div(expected_buyback_denominator)
        .ok_or(PricingError::Overflow)?;
    let expected = i64::try_from(expected).map_err(|_| PricingError::Overflow)?;
    // The house edge lives here, and only here.
    //
    // The contract costs `market_value / 0.92`, so the player's expected
    // return is 92% of what they paid and the house keeps 8%. Charging
    // the market value itself -- which is what this did before the
    // game-mechanics document fixed an explicit edge -- would leave a 0%
    // contract-time margin and make the whole house take depend on
    // players later selling back.
    //
    // Rounded up, deliberately: rounding must not fall on the player's
    // side of the edge, or the realised margin would sit below the
    // configured one by up to a microcredit on every contract.
    let quote_total = ceil_ratio(
        market_numerator
            .checked_mul(i128::from(BPS_DENOMINATOR))
            .ok_or(PricingError::Overflow)?,
        common_denominator
            .checked_mul(i128::from(TARGET_EV_BPS))
            .ok_or(PricingError::Overflow)?,
    )?;
    let adjustment = quote_total
        .checked_sub(verified_input_value_microcredits)
        .ok_or(PricingError::Overflow)?;
    let fee = adjustment.max(0);
    // A rebate is paid at the buyback rate, not at face value.
    //
    // Paid in full it would be a spread-free exit from the item economy:
    // contract valuable inputs into a cheap outcome set, take the
    // difference in credits at 100%, and keep the output item as well --
    // strictly better than selling, which pays 85%. The rebate is a
    // sell-back of the excess, so it is priced as one.
    let excess = adjustment.checked_neg().unwrap_or(0).max(0);
    let rebate = if excess == 0 {
        0
    } else {
        buyback_microcredits(excess)?
    };
    let effective_spread = if quote_total == 0 {
        Decimal::ZERO
    } else {
        let divisor = gcd_i128(expected_buyback_numerator, expected_buyback_denominator);
        let reduced_numerator = expected_buyback_numerator / divisor;
        let reduced_denominator = expected_buyback_denominator / divisor;
        let expected_decimal = Decimal::try_from_i128_with_scale(reduced_numerator, 0)
            .map_err(|_| PricingError::Overflow)?
            / Decimal::try_from_i128_with_scale(reduced_denominator, 0)
                .map_err(|_| PricingError::Overflow)?;
        Decimal::ONE - expected_decimal / Decimal::from(quote_total)
    };

    Ok(QuotePrice {
        expected_buyback_microcredits: expected,
        quote_total_microcredits: quote_total,
        adjustment_microcredits: adjustment,
        fee_microcredits: fee,
        rebate_microcredits: rebate,
        effective_spread,
    })
}

fn gcd_i128(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn checked_lcm_i128(a: i128, b: i128) -> Option<i128> {
    (a / gcd_i128(a, b)).checked_mul(b)
}

fn ceil_ratio(numerator: i128, denominator: i128) -> Result<i64, PricingError> {
    if denominator <= 0 || numerator < 0 {
        return Err(PricingError::InvalidProbabilities);
    }
    let value = numerator
        .checked_add(denominator - 1)
        .ok_or(PricingError::Overflow)?
        / denominator;
    i64::try_from(value).map_err(|_| PricingError::Overflow)
}

fn round_ratio_half_even(numerator: i128, denominator: i128) -> Result<i64, PricingError> {
    if denominator <= 0 || numerator < 0 {
        return Err(PricingError::Overflow);
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = remainder.checked_mul(2).ok_or(PricingError::Overflow)?;
    let rounded = if doubled > denominator || (doubled == denominator && quotient % 2 != 0) {
        quotient.checked_add(1).ok_or(PricingError::Overflow)?
    } else {
        quotient
    };
    i64::try_from(rounded).map_err(|_| PricingError::Overflow)
}
