use std::collections::BTreeMap;

use economy_core::tradeup::{
    InputItem, OutputItem, ScarcityMultiplier, TradeupError, apply_outcome_scarcity,
    build_outcomes, calculate_output_float, select_outcome, server_seed_commitment,
};
use rust_decimal_macros::dec;

/// A distinct item each time it is called, because a contract spends
/// specific instances and the same one may not be spent twice.
fn input(sku: &str, collection: &str, rarity: u8, value: rust_decimal::Decimal) -> InputItem {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    InputItem {
        item_id: format!("item-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
        sku_id: sku.into(),
        collection_id: collection.into(),
        rarity,
        float: value,
        min_float: dec!(0.10),
        max_float: dec!(0.50),
    }
}

fn output(sku: &str, collection: &str, rarity: u8, available: bool) -> OutputItem {
    OutputItem {
        sku_id: sku.into(),
        collection_id: collection.into(),
        rarity,
        available,
    }
}

/// A contract takes four to ten inputs inclusive. The old "exactly ten"
/// rule is gone; the bounds are what remain, and both edges are real.
#[test]
fn rejects_an_input_count_outside_four_to_ten() {
    for count in [0_usize, 1, 2, 3, 11, 20] {
        let inputs = (0..count)
            .map(|_| input("in", "a", 2, dec!(0.20)))
            .collect::<Vec<_>>();
        let error = build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err();
        assert_eq!(
            error,
            TradeupError::InputCount { actual: count },
            "{count} inputs must be refused"
        );
    }
}

#[test]
fn accepts_every_input_count_from_four_to_ten() {
    for count in 4_usize..=10 {
        let inputs = (0..count)
            .map(|_| input("in", "a", 2, dec!(0.20)))
            .collect::<Vec<_>>();
        let outcomes = build_outcomes(
            &inputs,
            &[output("out-a", "a", 3, true), output("out-b", "a", 3, true)],
        )
        .unwrap_or_else(|error| panic!("{count} inputs must be accepted, got {error:?}"));

        // Whatever the count, the weights still form a proper
        // distribution: one shared denominator that the numerators sum to.
        let denominator = outcomes[0].weight_denominator;
        assert!(
            outcomes
                .iter()
                .all(|outcome| outcome.weight_denominator == denominator),
            "{count} inputs: outcomes must share one denominator"
        );
        let total: u64 = outcomes
            .iter()
            .map(|outcome| outcome.weight_numerator)
            .sum();
        assert_eq!(
            total, denominator,
            "{count} inputs: numerators must sum to the denominator"
        );
    }
}

/// The worked example from the game-mechanics document: four inputs,
/// three from collection A and one from B, gives A 75% and B 25%.
/// Collection probability follows input composition, and that must hold
/// at any accepted count, not only at ten.
#[test]
fn collection_probability_follows_input_composition_at_any_count() {
    let mut inputs = (0..3)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.push(input("b-in", "b", 2, dec!(0.20)));

    let outcomes = build_outcomes(
        &inputs,
        &[output("a-out", "a", 3, true), output("b-out", "b", 3, true)],
    )
    .expect("four mixed inputs build outcomes");

    let denominator = outcomes[0].weight_denominator;
    let weight_of = |sku: &str| {
        outcomes
            .iter()
            .find(|outcome| outcome.sku_id == sku)
            .map(|outcome| outcome.weight_numerator)
            .unwrap_or_else(|| panic!("{sku} is among the outcomes"))
    };

    // Exact rational equality, not a rounded percentage: 3/4 and 1/4.
    assert_eq!(
        u128::from(weight_of("a-out")) * 4,
        u128::from(denominator) * 3,
        "collection A holds three of four inputs, so 75%"
    );
    assert_eq!(
        u128::from(weight_of("b-out")) * 4,
        u128::from(denominator),
        "collection B holds one of four inputs, so 25%"
    );
}

/// The output float averages over the inputs that were actually
/// submitted. Dividing by a hard-coded ten would understate the average
/// for every contract with fewer than ten inputs, quietly biasing every
/// result toward Factory New.
#[test]
fn output_float_averages_over_the_actual_input_count() {
    // Four inputs, all at the very top of their catalog range: the
    // normalized average is 1, so the output must sit at its own maximum.
    let inputs = (0..4)
        .map(|_| input("in", "a", 2, dec!(0.50)))
        .collect::<Vec<_>>();
    let float = calculate_output_float(&inputs, dec!(0.00), dec!(1.00))
        .expect("four inputs produce a float");
    assert_eq!(
        float,
        dec!(1.00000000),
        "averaging over ten instead of four would give 0.4"
    );

    // And at the bottom of the range, the output sits at its minimum.
    let inputs = (0..5)
        .map(|_| input("in", "a", 2, dec!(0.10)))
        .collect::<Vec<_>>();
    let float = calculate_output_float(&inputs, dec!(0.20), dec!(0.80))
        .expect("five inputs produce a float");
    assert_eq!(float, dec!(0.20000000));
}

#[test]
fn rejects_mixed_input_rarities() {
    let mut inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs[9].rarity = 3;
    let error = build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err();
    assert_eq!(error, TradeupError::MixedInputRarities);
}

#[test]
fn rejects_covert_inputs() {
    let inputs = (0..10)
        .map(|_| input("in", "a", 5, dec!(0.20)))
        .collect::<Vec<_>>();
    let error = build_outcomes(&inputs, &[output("out", "a", 6, true)]).unwrap_err();
    assert_eq!(error, TradeupError::CovertInput);
}

#[test]
fn assigns_collection_then_uniform_output_weights_exactly() {
    let mut inputs = (0..7)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..3)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-2", "a", 3, true),
            output("b-1", "b", 3, true),
            output("a-1", "a", 3, true),
        ],
    )
    .unwrap();

    assert_eq!(
        outcomes
            .iter()
            .map(|o| o.sku_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a-1", "a-2", "b-1"]
    );
    assert_eq!(
        outcomes
            .iter()
            .map(|o| (o.weight_numerator, o.weight_denominator))
            .collect::<Vec<_>>(),
        vec![(35, 100), (35, 100), (30, 100)]
    );
    assert_eq!(
        outcomes.iter().map(|o| o.weight_numerator).sum::<u64>(),
        100
    );
}

#[test]
fn normalized_input_floats_determine_output_float() {
    let mut inputs = (0..5)
        .map(|_| input("low", "a", 2, dec!(0.10)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..5)
            .map(|_| input("high", "a", 2, dec!(0.50)))
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        calculate_output_float(&inputs, dec!(0.20), dec!(0.80)).unwrap(),
        dec!(0.50)
    );
}

#[test]
fn a_zero_stock_outcome_is_excluded_and_the_rest_renormalised() {
    // The game-mechanics document: "Zero-stock outcome исключается.
    // Оставшиеся вероятности нормализуются." Refusing the whole contract
    // instead -- which is what this did before -- means one out-of-stock
    // candidate makes every contract touching that collection impossible.
    let inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    let outcomes = build_outcomes(
        &inputs,
        &[
            output("in-stock", "a", 3, true),
            output("out-of-stock", "a", 3, false),
        ],
    )
    .expect("an out-of-stock candidate is excluded, not fatal");

    assert_eq!(outcomes.len(), 1, "only the in-stock candidate survives");
    assert_eq!(outcomes[0].sku_id, "in-stock");
    assert_eq!(
        outcomes[0].weight_numerator, outcomes[0].weight_denominator,
        "the survivor takes the whole probability mass, exactly"
    );
}

/// Exclusion is not the same as tolerating an empty collection. If a
/// represented collection has nothing left in stock, there is no valid
/// outcome for those inputs and the contract must still be refused.
#[test]
fn a_collection_with_no_stock_left_is_still_refused() {
    let inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    let error = build_outcomes(&inputs, &[output("out", "a", 3, false)]).unwrap_err();
    assert_eq!(
        error,
        TradeupError::MissingOutput {
            collection_id: "a".into()
        }
    );
}

/// Excluding an outcome must not disturb the collection odds, which the
/// document ties to input composition. Three of A and one of B stays
/// 75/25 even when one of A's candidates is out of stock.
#[test]
fn exclusion_renormalises_within_a_collection_without_moving_collection_odds() {
    let mut inputs = (0..3)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.push(input("b-in", "b", 2, dec!(0.20)));

    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-live", "a", 3, true),
            output("a-dead", "a", 3, false),
            output("b-live", "b", 3, true),
        ],
    )
    .expect("build outcomes with one candidate out of stock");

    let denominator = outcomes[0].weight_denominator;
    let weight_of = |sku: &str| {
        outcomes
            .iter()
            .find(|outcome| outcome.sku_id == sku)
            .map(|outcome| outcome.weight_numerator)
            .unwrap_or_else(|| panic!("{sku} is among the outcomes"))
    };

    assert!(
        !outcomes.iter().any(|outcome| outcome.sku_id == "a-dead"),
        "the zero-stock candidate is gone"
    );
    assert_eq!(
        u128::from(weight_of("a-live")) * 4,
        u128::from(denominator) * 3,
        "collection A still holds three of four inputs, so still 75%"
    );
    assert_eq!(
        u128::from(weight_of("b-live")) * 4,
        u128::from(denominator),
        "and B still 25%"
    );
}

#[test]
fn commitment_hides_the_server_seed_and_selection_replays() {
    let seed = [7_u8; 32];
    assert_ne!(server_seed_commitment(&seed), seed);

    let inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-2", "a", 3, true), output("a-1", "a", 3, true)],
    )
    .unwrap();
    let first = select_outcome(&outcomes, &seed, b"browser-client-seed", 42).unwrap();
    let replay = select_outcome(&outcomes, &seed, b"browser-client-seed", 42).unwrap();
    assert_eq!(first, replay);
}

#[test]
fn rejects_rarity_that_cannot_have_a_next_tier_without_panicking() {
    let inputs = (0..10)
        .map(|_| input("in", "a", u8::MAX, dec!(0.20)))
        .collect::<Vec<_>>();
    assert_eq!(
        build_outcomes(&inputs, &[]).unwrap_err(),
        TradeupError::InvalidRarity { rarity: u8::MAX }
    );
}

#[test]
fn rejects_weight_sum_overflow_without_panicking() {
    use economy_core::tradeup::WeightedOutcome;
    let outcomes = [
        WeightedOutcome {
            sku_id: "a".into(),
            collection_id: "a".into(),
            weight_numerator: u64::MAX,
            weight_denominator: u64::MAX,
        },
        WeightedOutcome {
            sku_id: "b".into(),
            collection_id: "b".into(),
            weight_numerator: 1,
            weight_denominator: u64::MAX,
        },
    ];
    assert_eq!(
        select_outcome(&outcomes, &[0; 32], b"client", 1).unwrap_err(),
        TradeupError::InvalidWeights
    );
}

#[test]
fn scarcity_redistributes_within_a_collection_without_moving_collection_odds() {
    // The document ties collection odds to input composition and says
    // scarcity changes the weights of eligible outcomes *inside* the
    // chosen collection. Together those mean a scarcity multiplier must
    // never move probability between collections.
    let mut inputs = (0..3)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.push(input("b-in", "b", 2, dec!(0.20)));

    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-1", "a", 3, true),
            output("a-2", "a", 3, true),
            output("b-1", "b", 3, true),
        ],
    )
    .expect("build outcomes");

    // Damp one SKU inside collection A. B is untouched.
    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 4,
        },
    );
    let damped = apply_outcome_scarcity(&outcomes, &multipliers).expect("apply scarcity");

    let denominator = damped[0].weight_denominator;
    assert!(
        damped
            .iter()
            .all(|outcome| outcome.weight_denominator == denominator)
    );
    let share = |collection: &str| -> u128 {
        damped
            .iter()
            .filter(|outcome| outcome.collection_id == collection)
            .map(|outcome| u128::from(outcome.weight_numerator))
            .sum()
    };

    assert_eq!(
        share("a") * 4,
        u128::from(denominator) * 3,
        "collection A holds three of four inputs and must still be 75%"
    );
    assert_eq!(
        share("b") * 4,
        u128::from(denominator),
        "and B still 25%, untouched by a multiplier applied inside A"
    );

    let weight_of = |sku: &str| -> u128 {
        damped
            .iter()
            .find(|outcome| outcome.sku_id == sku)
            .map(|outcome| u128::from(outcome.weight_numerator))
            .unwrap_or_else(|| panic!("{sku} is among the outcomes"))
    };
    assert_eq!(
        weight_of("a-1") * 4,
        weight_of("a-2"),
        "a 1/4 multiplier means a quarter of its sibling's weight, exactly"
    );
}

#[test]
fn an_unmultiplied_outcome_set_keeps_every_share_exactly() {
    let mut inputs = (0..6)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..4)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("b-1", "b", 3, true)],
    )
    .expect("build outcomes");

    let damped = apply_outcome_scarcity(&outcomes, &BTreeMap::new()).expect("no multipliers");

    let denominator = damped[0].weight_denominator;
    for outcome in &outcomes {
        let after = damped
            .iter()
            .find(|candidate| candidate.sku_id == outcome.sku_id)
            .expect("every outcome survives");
        // Cross-multiplied, so this compares proportions rather than
        // whatever common denominator the rescale happened to pick.
        assert_eq!(
            u128::from(after.weight_numerator) * u128::from(outcome.weight_denominator),
            u128::from(outcome.weight_numerator) * u128::from(denominator),
            "{} must keep its exact share",
            outcome.sku_id
        );
    }
}

#[test]
fn an_outcome_damped_to_nothing_is_dropped_and_its_collection_renormalised() {
    let mut inputs = (0..5)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..5)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-1", "a", 3, true),
            output("a-2", "a", 3, true),
            output("b-1", "b", 3, true),
        ],
    )
    .expect("build outcomes");

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 0,
            denominator: 1,
        },
    );
    let damped = apply_outcome_scarcity(&outcomes, &multipliers).expect("apply scarcity");

    assert!(
        !damped.iter().any(|outcome| outcome.sku_id == "a-1"),
        "a fully depleted outcome is dropped rather than carried at zero"
    );

    let denominator = damped[0].weight_denominator;
    let share = |collection: &str| -> u128 {
        damped
            .iter()
            .filter(|outcome| outcome.collection_id == collection)
            .map(|outcome| u128::from(outcome.weight_numerator))
            .sum()
    };
    // The mass a-1 held goes to a-2, never to collection B.
    assert_eq!(
        share("a") * 2,
        u128::from(denominator),
        "collection A still holds five of ten inputs, so still 50%"
    );
    assert_eq!(share("b") * 2, u128::from(denominator));
}

#[test]
fn a_collection_damped_to_nothing_is_an_error_rather_than_a_silent_shift() {
    let mut inputs = (0..5)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..5)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("b-1", "b", 3, true)],
    )
    .expect("build outcomes");

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 0,
            denominator: 1,
        },
    );

    // Silently handing A's 50% to B would break the tie between collection
    // odds and input composition, which is the one thing scarcity must not
    // do. There is no correct answer here, so there is no answer.
    assert_eq!(
        apply_outcome_scarcity(&outcomes, &multipliers).unwrap_err(),
        TradeupError::MissingOutput {
            collection_id: "a".into()
        }
    );
}

#[test]
fn scarcity_rejects_an_invalid_multiplier_or_a_malformed_outcome_set() {
    let inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("a-2", "a", 3, true)],
    )
    .expect("build outcomes");

    // Above one would let scarcity *raise* a weight, which is not damping
    // and would move probability between collections.
    let mut invalid = BTreeMap::new();
    invalid.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 3,
            denominator: 2,
        },
    );
    assert_eq!(
        apply_outcome_scarcity(&outcomes, &invalid).unwrap_err(),
        TradeupError::InvalidWeights
    );

    let mut zero_denominator = BTreeMap::new();
    zero_denominator.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 0,
            denominator: 0,
        },
    );
    assert_eq!(
        apply_outcome_scarcity(&outcomes, &zero_denominator).unwrap_err(),
        TradeupError::InvalidWeights
    );

    // An empty set, and one whose members disagree about their
    // denominator, are malformed rather than merely unusual.
    assert_eq!(
        apply_outcome_scarcity(&[], &BTreeMap::new()).unwrap_err(),
        TradeupError::InvalidWeights
    );
    let mut mismatched = outcomes.clone();
    mismatched[1].weight_denominator += 1;
    assert_eq!(
        apply_outcome_scarcity(&mismatched, &BTreeMap::new()).unwrap_err(),
        TradeupError::InvalidWeights
    );
}

/// A damped distribution must remain something `select_outcome` accepts:
/// one shared denominator the numerators sum to, and no zero weight.
#[test]
fn a_damped_distribution_is_still_selectable() {
    let mut inputs = (0..7)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..3)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-1", "a", 3, true),
            output("a-2", "a", 3, true),
            output("a-3", "a", 3, true),
            output("b-1", "b", 3, true),
        ],
    )
    .expect("build outcomes");

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a-2".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 3,
        },
    );
    multipliers.insert(
        "b-1".to_string(),
        ScarcityMultiplier {
            numerator: 7,
            denominator: 10,
        },
    );
    let damped = apply_outcome_scarcity(&outcomes, &multipliers).expect("apply scarcity");

    let denominator = damped[0].weight_denominator;
    assert!(damped.iter().all(|outcome| outcome.weight_numerator > 0));
    assert_eq!(
        damped
            .iter()
            .map(|outcome| u128::from(outcome.weight_numerator))
            .sum::<u128>(),
        u128::from(denominator)
    );
    select_outcome(&damped, &[3_u8; 32], b"client", 1).expect("a damped set is selectable");
}

/// A five-collection contract with the multiplier denominators the stock
/// policy actually produces.
///
/// An earlier version took the lowest common multiple of the collections'
/// damped totals, which is a product of near-coprime numbers: this case
/// needed a 67-bit denominator and came back as `InvalidWeights`, so an
/// ordinary multi-collection trade-up simply failed, and the error blamed
/// the data rather than an internal range limit.
#[test]
fn a_multi_collection_contract_with_awkward_denominators_still_prices() {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for collection in 0..5 {
        let id = format!("c{collection}");
        inputs.extend(
            (0..2)
                .map(|_| input("in", &id, 2, dec!(0.20)))
                .collect::<Vec<_>>(),
        );
        for candidate in 0..2 {
            outputs.push(output(&format!("c{collection}-{candidate}"), &id, 3, true));
        }
    }

    let outcomes = build_outcomes(&inputs, &outputs).expect("ten inputs over five collections");

    // Denominators taken from the shape `publish_collection_scarcity_snapshot`
    // writes: available over target, with targets from the rarity policy.
    let denominators = [167_u64, 125, 83, 42, 17, 7, 199, 151, 113, 61];
    let mut multipliers = BTreeMap::new();
    for (index, outcome) in outcomes.iter().enumerate() {
        let denominator = denominators[index % denominators.len()];
        multipliers.insert(
            outcome.sku_id.clone(),
            ScarcityMultiplier {
                numerator: denominator - 1,
                denominator,
            },
        );
    }

    let damped = apply_outcome_scarcity(&outcomes, &multipliers)
        .expect("awkward denominators must not exhaust the weight range");

    // Each collection holds two of ten inputs, so each must still hold
    // exactly a fifth -- the rounding happens strictly inside a
    // collection, never across them.
    let denominator = u128::from(damped[0].weight_denominator);
    for collection in 0..5 {
        let id = format!("c{collection}");
        let share: u128 = damped
            .iter()
            .filter(|outcome| outcome.collection_id == id)
            .map(|outcome| u128::from(outcome.weight_numerator))
            .sum();
        assert_eq!(
            share * 5,
            denominator,
            "collection {id} holds two of ten inputs and must still be a fifth"
        );
    }

    select_outcome(&damped, &[9_u8; 32], b"client", 7).expect("and the result is selectable");
}

/// Two runs over the same inputs must assign the same leftover units, or a
/// quote is not reproducible.
#[test]
fn apportioning_leftovers_is_deterministic() {
    let mut inputs = (0..7)
        .map(|_| input("a-in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    inputs.extend(
        (0..3)
            .map(|_| input("b-in", "b", 2, dec!(0.20)))
            .collect::<Vec<_>>(),
    );
    let outputs = [
        output("a-1", "a", 3, true),
        output("a-2", "a", 3, true),
        output("a-3", "a", 3, true),
        output("b-1", "b", 3, true),
    ];
    let outcomes = build_outcomes(&inputs, &outputs).expect("build outcomes");

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a-1".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 3,
        },
    );
    multipliers.insert(
        "a-2".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 7,
        },
    );

    let first = apply_outcome_scarcity(&outcomes, &multipliers).expect("first run");
    let second = apply_outcome_scarcity(&outcomes, &multipliers).expect("second run");
    assert_eq!(first, second, "the same inputs must give the same weights");
}

/// The same physical item cannot be spent twice.
///
/// Enforced here as well as in `finalize_contract`, because this is the
/// layer that would otherwise compute a distribution and an output float
/// from a set of inputs that cannot exist.
#[test]
fn the_same_item_cannot_be_used_twice() {
    let mut inputs: Vec<_> = (0..4).map(|_| input("in", "a", 2, dec!(0.20))).collect();
    inputs[3] = inputs[0].clone();

    let error = build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err();
    assert_eq!(
        error,
        TradeupError::DuplicateInput {
            item_id: inputs[0].item_id.clone()
        }
    );

    // The float average is computed from the same validated set, so it
    // refuses too rather than averaging a duplicate.
    assert!(calculate_output_float(&inputs, dec!(0.00), dec!(1.00)).is_err());
}

/// Selection must depend on the outcome set, never on the order it was
/// passed in.
///
/// The only replay test used the same slice in the same order twice, so it
/// could not see this. Without the sort, the same seeds and nonce would
/// pick a different item -- and publish a different digest -- depending on
/// whether the caller used database order or sorted order, and a player
/// verifying the commitment would correctly conclude the house cheated.
#[test]
fn selection_does_not_depend_on_the_order_outcomes_are_given_in() {
    let inputs = (0..10)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    let outcomes = build_outcomes(
        &inputs,
        &[
            output("a-1", "a", 3, true),
            output("a-2", "a", 3, true),
            output("a-3", "a", 3, true),
            output("a-4", "a", 3, true),
            output("a-5", "a", 3, true),
        ],
    )
    .expect("build outcomes");

    let seed = [42_u8; 32];
    let mut reversed = outcomes.clone();
    reversed.reverse();
    let mut rotated = outcomes.clone();
    rotated.rotate_left(2);

    for nonce in 0..50 {
        let forward = select_outcome(&outcomes, &seed, b"client", nonce).expect("forward");
        for other in [&reversed, &rotated] {
            let permuted = select_outcome(other, &seed, b"client", nonce).expect("permuted");
            assert_eq!(forward.sku_id, permuted.sku_id, "nonce {nonce}: same item");
            assert_eq!(
                forward.digest, permuted.digest,
                "nonce {nonce}: the published digest must not depend on input order either"
            );
        }
    }
}

/// These three guards are what stop a corrupt catalog row from becoming a
/// signed quote. Each was mutation-confirmed untested: deleting it left the
/// whole suite green.
#[test]
fn an_input_float_outside_its_catalog_range_is_refused() {
    let mut inputs = (0..4)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    // The helper's range is 0.10..=0.50, so 0.60 cannot be this item.
    inputs[0].float = dec!(0.60);
    let expected = TradeupError::FloatOutOfRange {
        sku_id: inputs[0].sku_id.clone(),
    };
    assert_eq!(
        build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err(),
        expected
    );
    assert_eq!(
        calculate_output_float(&inputs, dec!(0.00), dec!(1.00)).unwrap_err(),
        expected,
        "the float calculation must refuse it too, not average an impossible value"
    );
}

#[test]
fn a_degenerate_catalog_float_range_is_refused() {
    // min == max would make the normalisation divide by zero; min > max
    // would invert it. Neither is a real item.
    for (min, max) in [(dec!(0.30), dec!(0.30)), (dec!(0.40), dec!(0.20))] {
        let mut inputs = (0..4)
            .map(|_| input("in", "a", 2, dec!(0.20)))
            .collect::<Vec<_>>();
        inputs[1].min_float = min;
        inputs[1].max_float = max;
        assert_eq!(
            build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err(),
            TradeupError::InvalidFloatRange {
                sku_id: inputs[1].sku_id.clone()
            },
            "a range of {min}..{max} must be refused"
        );
    }
}

#[test]
fn a_candidate_of_the_wrong_rarity_tier_is_refused() {
    // Inputs are rarity 2, so the only valid tier for a result is 3.
    let inputs = (0..4)
        .map(|_| input("in", "a", 2, dec!(0.20)))
        .collect::<Vec<_>>();
    for wrong_tier in [2_u8, 4] {
        assert_eq!(
            build_outcomes(&inputs, &[output("skip-a-tier", "a", wrong_tier, true)]).unwrap_err(),
            TradeupError::WrongOutputRarity {
                sku_id: "skip-a-tier".into()
            },
            "a tier-{wrong_tier} candidate must not be awarded for tier-2 inputs"
        );
    }
}
