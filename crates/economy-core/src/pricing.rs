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
    let denominator = outcomes
        .first()
        .map(|outcome| outcome.weight_denominator)
        .filter(|value| *value > 0)
        .ok_or(PricingError::InvalidProbabilities)?;
    let weight_sum = outcomes.iter().try_fold(0_u64, |sum, outcome| {
        if outcome.weight_denominator != denominator || outcome.weight_numerator == 0 {
            return Err(PricingError::InvalidProbabilities);
        }
        sum.checked_add(outcome.weight_numerator)
            .ok_or(PricingError::InvalidProbabilities)
    })?;
    if weight_sum != denominator {
        return Err(PricingError::InvalidProbabilities);
    }

    let expected_numerator = outcomes.iter().try_fold(0_i128, |sum, outcome| {
        let buyback = buyback_microcredits(outcome.verified_price_microcredits)?;
        let weighted = i128::from(buyback)
            .checked_mul(i128::from(outcome.weight_numerator))
            .ok_or(PricingError::Overflow)?;
        sum.checked_add(weighted).ok_or(PricingError::Overflow)
    })?;
    let expected = round_ratio_half_even(expected_numerator, i128::from(denominator))?;
    let quote_numerator = expected_numerator
        .checked_mul(i128::from(PERCENT_DENOMINATOR))
        .ok_or(PricingError::Overflow)?;
    let quote_denominator = i128::from(denominator)
        .checked_mul(i128::from(BUYBACK_PERCENT))
        .ok_or(PricingError::Overflow)?;
    let quote_total = ceil_ratio(quote_numerator, quote_denominator)?;
    let adjustment = quote_total
        .checked_sub(verified_input_value_microcredits)
        .ok_or(PricingError::Overflow)?;
    let fee = adjustment.max(0);
    let rebate = adjustment.checked_neg().unwrap_or(0).max(0);
    let effective_spread = if quote_total == 0 {
        Decimal::ZERO
    } else {
        Decimal::ONE - Decimal::from(expected) / Decimal::from(quote_total)
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
