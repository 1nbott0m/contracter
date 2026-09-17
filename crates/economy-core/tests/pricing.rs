use economy_core::pricing::{
    HOUSE_EDGE_BPS, MINIMUM_ITEM_VALUE_MICROCREDITS, PricedOutcome, PricingError, TARGET_EV_BPS,
    buyback_microcredits, quote_adjustment_microcredits, trimmed_mean_microcredits,
    validate_item_value,
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

    // Market value is 1_500_000; the contract costs that over 0.92, so
    // the house keeps 8% of what the player pays.
    assert_eq!(quote.quote_total_microcredits, 1_630_435);
    // The buyback figure is untouched by the edge: it is still 85% of the
    // market value, because it describes selling the result, not buying
    // the contract.
    assert_eq!(quote.expected_buyback_microcredits, 1_275_000);
    assert_eq!(quote.adjustment_microcredits, 230_435);
    assert_eq!(quote.fee_microcredits, 230_435);
    assert_eq!(quote.rebate_microcredits, 0);
    // The spread a player actually faces is now both layers compounded:
    // 1 - 0.85 * 0.92 = 0.218.
    assert_eq!(quote.effective_spread.round_dp(3), dec!(0.218));
}

#[test]
fn quote_rounds_only_after_exact_weighted_market_value() {
    let outcomes = [PricedOutcome::new(101, 1, 1)];
    let quote = quote_adjustment_microcredits(101, &outcomes).unwrap();
    assert_eq!(quote.expected_buyback_microcredits, 86);
    // 101 / 0.92 = 109.78..., rounded up. The division happens once, on
    // the exact weighted value, never on already-rounded parts.
    assert_eq!(quote.quote_total_microcredits, 110);
}

#[test]
fn quote_accepts_equivalent_mixed_probability_denominators() {
    let outcomes = [
        PricedOutcome::new(600, 1, 2),
        PricedOutcome::new(600, 1, 3),
        PricedOutcome::new(600, 1, 6),
    ];
    // 600 / 0.92 = 652.17..., rounded up. Equivalent denominators must
    // still reduce to one exact market value before the edge is applied.
    assert_eq!(
        quote_adjustment_microcredits(600, &outcomes)
            .unwrap()
            .quote_total_microcredits,
        653
    );
}

#[test]
fn quote_returns_rebate_without_a_simultaneous_fee() {
    // A rebate still happens, but the edge means the inputs must now be
    // worth more than the outcome's market value before one appears:
    // 1_000_000 / 0.92 = 1_086_957, so 1_200_000 of inputs earns 113_043
    // back rather than the whole difference.
    let outcomes = [PricedOutcome::new(1_000_000, 1, 1)];
    let quote = quote_adjustment_microcredits(1_200_000, &outcomes).unwrap();
    assert_eq!(quote.quote_total_microcredits, 1_086_957);
    assert_eq!(quote.adjustment_microcredits, -113_043);
    assert_eq!(quote.fee_microcredits, 0);
    assert_eq!(quote.rebate_microcredits, 113_043);

    // And inputs merely equal to the market value now owe a fee, because
    // the contract costs more than the distribution is worth.
    let quote = quote_adjustment_microcredits(1_000_000, &outcomes).unwrap();
    assert_eq!(quote.fee_microcredits, 86_957);
    assert_eq!(quote.rebate_microcredits, 0);
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
    assert_eq!(quote.quote_total_microcredits, 1_086_956_522);
    assert_eq!(quote.effective_spread.round_dp(3), dec!(0.218));
}

/// The game-mechanics document fixes the house edge at 8%, so a contract's
/// expected value is 92% of what the player pays for it.
///
/// This is the *contract-time* edge and is deliberately separate from the
/// 15% buyback spread, which is the *sell-back* spread and applies only
/// when a player converts an item to credits. Conflating the two would
/// take margin twice.
#[test]
fn the_quote_total_places_the_expected_value_at_ninety_two_percent() {
    // One certain outcome worth 368 credits: the document's own worked
    // example, read backwards. A player must pay 400 for it.
    let outcomes = [PricedOutcome::new(368_000_000, 1, 1)];
    let price = quote_adjustment_microcredits(0, &outcomes).expect("price the quote");

    assert_eq!(
        price.quote_total_microcredits, 400_000_000,
        "368 is 92% of 400, so 400 is what the contract costs"
    );

    // Stated as the invariant rather than the arithmetic: the expected
    // market value is TARGET_EV_BPS of the total, to within the one
    // microcredit that ceiling rounding can add.
    let market_value = 368_000_000_i128;
    let implied = i128::from(price.quote_total_microcredits) * i128::from(TARGET_EV_BPS);
    let exact = market_value * 10_000;
    assert!(
        (implied - exact).abs() <= i128::from(TARGET_EV_BPS),
        "expected value must land on 92% of the total, got {implied} vs {exact}"
    );
}

#[test]
fn the_house_edge_and_the_target_expected_value_are_complements() {
    assert_eq!(HOUSE_EDGE_BPS, 800, "8%");
    assert_eq!(TARGET_EV_BPS, 9_200, "92%");
    assert_eq!(
        HOUSE_EDGE_BPS + TARGET_EV_BPS,
        10_000,
        "the edge and the target must not be able to drift apart"
    );
}

/// Rounding must never fall on the player's side of the edge: the total
/// is rounded up, so the realised edge is at least the configured one.
#[test]
fn rounding_the_quote_total_never_shrinks_the_house_edge() {
    for market_value in [1_i64, 7, 999, 1_000_001, 123_456_789] {
        let outcomes = [PricedOutcome::new(market_value, 1, 1)];
        let price = quote_adjustment_microcredits(0, &outcomes).expect("price the quote");
        let realised_ev = i128::from(market_value) * 10_000;
        let target_ev = i128::from(price.quote_total_microcredits) * i128::from(TARGET_EV_BPS);
        assert!(
            realised_ev <= target_ev,
            "market value {market_value}: the player's expected value must not exceed 92% of the total"
        );
    }
}

/// An item below the minimum tradeable value cannot enter a contract.
#[test]
fn items_below_the_minimum_value_are_refused() {
    assert_eq!(
        MINIMUM_ITEM_VALUE_MICROCREDITS, 20_000_000,
        "twenty credits, in microcredits"
    );

    // At the boundary, accepted; one microcredit below, refused.
    assert_eq!(validate_item_value(MINIMUM_ITEM_VALUE_MICROCREDITS), Ok(()));
    assert_eq!(
        validate_item_value(MINIMUM_ITEM_VALUE_MICROCREDITS - 1),
        Err(PricingError::ItemBelowMinimumValue {
            value_microcredits: MINIMUM_ITEM_VALUE_MICROCREDITS - 1
        })
    );
    assert_eq!(
        validate_item_value(0),
        Err(PricingError::ItemBelowMinimumValue {
            value_microcredits: 0
        })
    );
    // A negative price is not merely cheap; it is not a price.
    assert_eq!(validate_item_value(-1), Err(PricingError::NegativeAmount));
}
