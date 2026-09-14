use economy_core::policy::{
    Coverage, InitialStockTargets, RiskPolicy, StockBand, coverage_ratio,
    quote_exposure_microcredits, validate_stock_bands,
};
use rust_decimal_macros::dec;

#[test]
fn mvp_stock_targets_match_the_approved_rarity_policy() {
    let targets = InitialStockTargets::mvp_v1();

    assert_eq!(targets.consumer, 167);
    assert_eq!(targets.industrial, 125);
    assert_eq!(targets.mil_spec, 83);
    assert_eq!(targets.restricted, 42);
    assert_eq!(targets.classified, 17);
    assert_eq!(targets.covert, 7);
}

#[test]
fn risk_policy_v1_matches_the_approved_limits() {
    let policy = RiskPolicy::mvp_v1();

    assert_eq!(policy.minimum_coverage_ratio, dec!(1.25));
    assert_eq!(policy.maximum_quote_reserve_ratio, dec!(0.01));
    assert_eq!(policy.maximum_item_liability_ratio, dec!(0.10));
    assert_eq!(policy.maximum_collection_liability_ratio, dec!(0.25));
    assert!(policy.validate().is_ok());
}

#[test]
fn zero_liability_has_infinite_coverage() {
    assert_eq!(coverage_ratio(0, 0).unwrap(), Coverage::Infinite);
    assert_eq!(coverage_ratio(50, 0).unwrap(), Coverage::Infinite);
}

#[test]
fn nonzero_liability_returns_an_exact_decimal_ratio() {
    assert_eq!(
        coverage_ratio(125, 100).unwrap(),
        Coverage::Finite(dec!(1.25))
    );
}

#[test]
fn quote_exposure_uses_maximum_buyback_plus_maximum_rebate() {
    assert_eq!(
        quote_exposure_microcredits(&[800, 1_200, 950], &[0, 75, 20]).unwrap(),
        1_275
    );
}

#[test]
fn quote_exposure_rejects_empty_or_negative_inputs_and_overflow() {
    assert!(quote_exposure_microcredits(&[], &[]).is_err());
    assert!(quote_exposure_microcredits(&[100, 200], &[10]).is_err());
    assert!(quote_exposure_microcredits(&[1], &[-1]).is_err());
    assert!(quote_exposure_microcredits(&[i64::MAX], &[1]).is_err());
}

#[test]
fn stock_bands_accept_adjacent_non_overlapping_ranges() {
    let bands = [
        StockBand::new(dec!(0), dec!(0.10), dec!(1.50)),
        StockBand::new(dec!(0.10), dec!(0.20), dec!(1.40)),
        StockBand::new(dec!(0.20), dec!(0.40), dec!(1.25)),
        StockBand::new(dec!(0.40), dec!(0.70), dec!(1.10)),
        StockBand::new(dec!(0.70), dec!(1), dec!(1.00)),
    ];

    assert!(validate_stock_bands(&bands).is_ok());
}

#[test]
fn stock_bands_reject_overlap_and_ratios_outside_zero_to_one() {
    let overlap = [
        StockBand::new(dec!(0), dec!(0.20), dec!(1.50)),
        StockBand::new(dec!(0.10), dec!(0.40), dec!(1.25)),
    ];
    let outside = [StockBand::new(dec!(-0.01), dec!(1), dec!(1.00))];

    assert!(validate_stock_bands(&overlap).is_err());
    assert!(validate_stock_bands(&outside).is_err());
}

#[test]
fn risk_policy_rejects_ratios_outside_zero_to_one() {
    let invalid = RiskPolicy {
        maximum_quote_reserve_ratio: dec!(1.01),
        ..RiskPolicy::mvp_v1()
    };

    assert!(invalid.validate().is_err());
}
