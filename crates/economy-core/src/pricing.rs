use rust_decimal::Decimal;
use thiserror::Error;

const BUYBACK_PERCENT: i64 = 85;
const PERCENT_DENOMINATOR: i64 = 100;
const MIN_SALES: usize = 20;

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
        if outcome.verified_price_microcredits < 0 {
            return Err(PricingError::NegativeAmount);
        }
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
    let expected = round_ratio_half_even(expected_buyback_numerator, expected_buyback_denominator)?;
    let quote_total = ceil_ratio(market_numerator, common_denominator)?;
    let adjustment = quote_total
        .checked_sub(verified_input_value_microcredits)
        .ok_or(PricingError::Overflow)?;
    let fee = adjustment.max(0);
    let rebate = adjustment.checked_neg().unwrap_or(0).max(0);
    let effective_spread = if quote_total == 0 {
        Decimal::ZERO
    } else {
        let expected_decimal =
            Decimal::from(expected_buyback_numerator) / Decimal::from(expected_buyback_denominator);
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
