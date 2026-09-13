use economy_core::pricing::{
    PricedOutcome, PricingError, buyback_microcredits, quote_adjustment_microcredits,
    trimmed_mean_microcredits,
};
use rust_decimal_macros::dec;

#[test]
fn ten_percent_trim_drops_two_values_from_each_end_of_twenty_sales() {
    let sales = (1_i64..=20).collect::<Vec<_>>();
    assert_eq!(trimmed_mean_microcredits(&sales).unwrap(), 10);
}

#[test]
fn valuation_rejects_fewer_than_twenty_sales() {
    assert_eq!(
        trimmed_mean_microcredits(&[100; 19]).unwrap_err(),
        PricingError::InsufficientSales { actual: 19 }
    );
}

#[test]
fn buyback_is_exactly_eighty_five_percent_rounded_down() {
    assert_eq!(buyback_microcredits(1_001_000).unwrap(), 850_850);
    assert_eq!(
        buyback_microcredits(-1).unwrap_err(),
        PricingError::NegativeAmount
    );
}

#[test]
fn quote_returns_fee_and_actual_effective_spread() {
    let outcomes = [
        PricedOutcome::new(1_000_000, 1, 2),
        PricedOutcome::new(2_000_000, 1, 2),
    ];
    let quote = quote_adjustment_microcredits(1_400_000, &outcomes).unwrap();
    assert_eq!(quote.expected_buyback_microcredits, 1_275_000);
    assert_eq!(quote.quote_total_microcredits, 1_500_000);
    assert_eq!(quote.adjustment_microcredits, 100_000);
    assert_eq!(quote.fee_microcredits, 100_000);
    assert_eq!(quote.rebate_microcredits, 0);
    assert_eq!(quote.effective_spread, dec!(0.15));
}

#[test]
fn quote_rounds_only_after_exact_weighted_market_value() {
    let outcomes = [PricedOutcome::new(101, 1, 1)];
    let quote = quote_adjustment_microcredits(101, &outcomes).unwrap();
    assert_eq!(quote.expected_buyback_microcredits, 86);
    assert_eq!(quote.quote_total_microcredits, 101);
    assert_eq!(quote.effective_spread, dec!(0.15));
}

#[test]
fn quote_accepts_equivalent_mixed_probability_denominators() {
    let outcomes = [
        PricedOutcome::new(600, 1, 2),
        PricedOutcome::new(600, 1, 3),
        PricedOutcome::new(600, 1, 6),
    ];
    assert_eq!(
        quote_adjustment_microcredits(600, &outcomes)
            .unwrap()
            .quote_total_microcredits,
        600
    );
}

#[test]
fn quote_returns_rebate_without_a_simultaneous_fee() {
    let outcomes = [PricedOutcome::new(1_000_000, 1, 1)];
    let quote = quote_adjustment_microcredits(1_100_000, &outcomes).unwrap();
    assert_eq!(quote.adjustment_microcredits, -100_000);
    assert_eq!(quote.fee_microcredits, 0);
    assert_eq!(quote.rebate_microcredits, 100_000);
}

#[test]
fn quote_rejects_probabilities_that_do_not_sum_to_one() {
    let outcomes = [PricedOutcome::new(1_000_000, 1, 2)];
    assert_eq!(
        quote_adjustment_microcredits(1_000_000, &outcomes).unwrap_err(),
        PricingError::InvalidProbabilities
    );
}

#[test]
fn quote_rejects_checked_arithmetic_overflow() {
    let outcomes = [PricedOutcome::new(i64::MAX, u64::MAX, u64::MAX)];
    assert_eq!(
        quote_adjustment_microcredits(0, &outcomes).unwrap_err(),
        PricingError::Overflow
    );
}

#[test]
fn large_equivalent_probability_does_not_panic_during_spread_calculation() {
    let outcomes = [PricedOutcome::new(1_000_000_000, u64::MAX, u64::MAX)];
    let quote = quote_adjustment_microcredits(1_000_000_000, &outcomes).unwrap();
    assert_eq!(quote.quote_total_microcredits, 1_000_000_000);
    assert_eq!(quote.effective_spread, dec!(0.15));
}
