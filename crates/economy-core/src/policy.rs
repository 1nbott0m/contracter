use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitialStockTargets {
    pub consumer: u32,
    pub industrial: u32,
    pub mil_spec: u32,
    pub restricted: u32,
    pub classified: u32,
    pub covert: u32,
}

impl InitialStockTargets {
    pub const fn mvp_v1() -> Self {
        Self {
            consumer: 167,
            industrial: 125,
            mil_spec: 83,
            restricted: 42,
            classified: 17,
            covert: 7,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskPolicy {
    pub minimum_coverage_ratio: Decimal,
    pub maximum_quote_reserve_ratio: Decimal,
    pub maximum_item_liability_ratio: Decimal,
    pub maximum_collection_liability_ratio: Decimal,
}

impl RiskPolicy {
    pub fn mvp_v1() -> Self {
        Self {
            minimum_coverage_ratio: Decimal::new(125, 2),
            maximum_quote_reserve_ratio: Decimal::new(1, 2),
            maximum_item_liability_ratio: Decimal::new(10, 2),
            maximum_collection_liability_ratio: Decimal::new(25, 2),
        }
    }

    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.minimum_coverage_ratio < Decimal::ONE {
            return Err(PolicyError::InvalidCoverageRatio);
        }
        for ratio in [
            self.maximum_quote_reserve_ratio,
            self.maximum_item_liability_ratio,
            self.maximum_collection_liability_ratio,
        ] {
            validate_unit_ratio(ratio)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StockBand {
    pub minimum_ratio: Decimal,
    pub maximum_ratio: Decimal,
    pub multiplier: Decimal,
}

impl StockBand {
    pub const fn new(minimum_ratio: Decimal, maximum_ratio: Decimal, multiplier: Decimal) -> Self {
        Self {
            minimum_ratio,
            maximum_ratio,
            multiplier,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coverage {
    Infinite,
    Finite(Decimal),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("a ratio must be between zero and one")]
    RatioOutOfRange,
    #[error("minimum coverage ratio must be at least one")]
    InvalidCoverageRatio,
    #[error("stock bands must cover 0..=1 exactly without gaps or overlap")]
    InvalidStockBands,
    #[error("stock multiplier must be between one and one and a half")]
    InvalidStockMultiplier,
    #[error("monetary values must be non-negative")]
    NegativeAmount,
    #[error("at least one quote outcome is required")]
    EmptyOutcomes,
    #[error("every quote outcome must have both a buyback and rebate value")]
    MismatchedOutcomes,
    #[error("monetary arithmetic overflowed")]
    AmountOverflow,
}

pub fn coverage_ratio(
    liquid_reserve_microcredits: i64,
    stressed_liability_microcredits: i64,
) -> Result<Coverage, PolicyError> {
    if liquid_reserve_microcredits < 0 || stressed_liability_microcredits < 0 {
        return Err(PolicyError::NegativeAmount);
    }
    if stressed_liability_microcredits == 0 {
        return Ok(Coverage::Infinite);
    }

    Ok(Coverage::Finite(
        Decimal::from(liquid_reserve_microcredits) / Decimal::from(stressed_liability_microcredits),
    ))
}

pub fn quote_exposure_microcredits(
    candidate_buybacks: &[i64],
    candidate_rebates: &[i64],
) -> Result<i64, PolicyError> {
    if candidate_buybacks.is_empty() || candidate_rebates.is_empty() {
        return Err(PolicyError::EmptyOutcomes);
    }
    if candidate_buybacks.len() != candidate_rebates.len() {
        return Err(PolicyError::MismatchedOutcomes);
    }
    if candidate_buybacks
        .iter()
        .chain(candidate_rebates)
        .any(|value| *value < 0)
    {
        return Err(PolicyError::NegativeAmount);
    }

    let maximum_buyback = *candidate_buybacks.iter().max().expect("checked non-empty");
    let maximum_rebate = *candidate_rebates.iter().max().expect("checked non-empty");
    maximum_buyback
        .checked_add(maximum_rebate)
        .ok_or(PolicyError::AmountOverflow)
}

pub fn validate_stock_bands(bands: &[StockBand]) -> Result<(), PolicyError> {
    if bands.is_empty()
        || bands[0].minimum_ratio != Decimal::ZERO
        || bands.last().expect("checked non-empty").maximum_ratio != Decimal::ONE
    {
        return Err(PolicyError::InvalidStockBands);
    }

    for (index, band) in bands.iter().enumerate() {
        validate_unit_ratio(band.minimum_ratio)?;
        validate_unit_ratio(band.maximum_ratio)?;
        if band.minimum_ratio >= band.maximum_ratio {
            return Err(PolicyError::InvalidStockBands);
        }
        if !(Decimal::ONE..=Decimal::new(15, 1)).contains(&band.multiplier) {
            return Err(PolicyError::InvalidStockMultiplier);
        }
        if index > 0 && bands[index - 1].maximum_ratio != band.minimum_ratio {
            return Err(PolicyError::InvalidStockBands);
        }
    }
    Ok(())
}

fn validate_unit_ratio(ratio: Decimal) -> Result<(), PolicyError> {
    if !(Decimal::ZERO..=Decimal::ONE).contains(&ratio) {
        return Err(PolicyError::RatioOutOfRange);
    }
    Ok(())
}
