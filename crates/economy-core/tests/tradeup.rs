use std::collections::BTreeMap;

use economy_core::tradeup::{
    InputItem, OutputItem, ScarcityMultiplier, TradeupError, apply_collection_scarcity,
    build_outcomes, calculate_output_float, select_outcome, server_seed_commitment,
};
use rust_decimal_macros::dec;

fn input(sku: &str, collection: &str, rarity: u8, value: rust_decimal::Decimal) -> InputItem {
    InputItem {
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

#[test]
fn rejects_any_input_count_other_than_ten() {
    let inputs = vec![input("in", "a", 2, dec!(0.20)); 9];
    let error = build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err();
    assert_eq!(error, TradeupError::InputCount { actual: 9 });
}

#[test]
fn rejects_mixed_input_rarities() {
    let mut inputs = vec![input("in", "a", 2, dec!(0.20)); 10];
    inputs[9].rarity = 3;
    let error = build_outcomes(&inputs, &[output("out", "a", 3, true)]).unwrap_err();
    assert_eq!(error, TradeupError::MixedInputRarities);
}

#[test]
fn rejects_covert_inputs() {
    let inputs = vec![input("in", "a", 5, dec!(0.20)); 10];
    let error = build_outcomes(&inputs, &[output("out", "a", 6, true)]).unwrap_err();
    assert_eq!(error, TradeupError::CovertInput);
}

#[test]
fn assigns_collection_then_uniform_output_weights_exactly() {
    let mut inputs = vec![input("a-in", "a", 2, dec!(0.20)); 7];
    inputs.extend(vec![input("b-in", "b", 2, dec!(0.20)); 3]);
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
    let mut inputs = vec![input("low", "a", 2, dec!(0.10)); 5];
    inputs.extend(vec![input("high", "a", 2, dec!(0.50)); 5]);
    assert_eq!(
        calculate_output_float(&inputs, dec!(0.20), dec!(0.80)).unwrap(),
        dec!(0.50)
    );
}

#[test]
fn rejects_an_unavailable_candidate_instead_of_rerolling() {
    let inputs = vec![input("in", "a", 2, dec!(0.20)); 10];
    let error = build_outcomes(&inputs, &[output("out", "a", 3, false)]).unwrap_err();
    assert_eq!(
        error,
        TradeupError::UnavailableOutput {
            sku_id: "out".into()
        }
    );
}

#[test]
fn commitment_hides_the_server_seed_and_selection_replays() {
    let seed = [7_u8; 32];
    assert_ne!(server_seed_commitment(&seed), seed);

    let inputs = vec![input("in", "a", 2, dec!(0.20)); 10];
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
    let inputs = vec![input("in", "a", u8::MAX, dec!(0.20)); 10];
    assert_eq!(
        build_outcomes(&inputs, &[]).unwrap_err(),
        TradeupError::InvalidRarity { rarity: u8::MAX }
    );
}

#[test]
fn scarcity_multiplier_scales_a_single_collections_weight_exactly() {
    let inputs = vec![input("in", "a", 2, dec!(0.20)); 10];
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("a-2", "a", 3, true)],
    )
    .unwrap();
    assert_eq!(outcomes[0].weight_numerator, 50);
    assert_eq!(outcomes[0].weight_denominator, 100);

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 2,
        },
    );
    let damped = apply_collection_scarcity(&outcomes, &multipliers).unwrap();

    assert_eq!(damped.len(), 2);
    assert!(damped.iter().all(|outcome| outcome.weight_numerator == 25));
    assert_eq!(damped[0].weight_denominator, 50);
    assert_eq!(
        damped.iter().map(|o| o.weight_numerator).sum::<u64>(),
        damped[0].weight_denominator
    );
}

#[test]
fn undamped_collections_keep_their_relative_share() {
    let mut inputs = vec![input("a-in", "a", 2, dec!(0.20)); 5];
    inputs.extend(vec![input("b-in", "b", 2, dec!(0.20)); 5]);
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("b-1", "b", 3, true)],
    )
    .unwrap();
    let baseline_b = outcomes
        .iter()
        .find(|o| o.collection_id == "b")
        .unwrap()
        .weight_numerator;
    let baseline_denominator = outcomes[0].weight_denominator;

    // "a" is damped; "b" is absent from the map and defaults to 1/1.
    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a".to_string(),
        ScarcityMultiplier {
            numerator: 1,
            denominator: 4,
        },
    );
    let damped = apply_collection_scarcity(&outcomes, &multipliers).unwrap();
    let damped_b = damped.iter().find(|o| o.collection_id == "b").unwrap();
    let damped_total = damped[0].weight_denominator;

    // b's share of the total strictly increases (cross-multiplied to avoid floats).
    assert!(
        (damped_b.weight_numerator as u128) * (baseline_denominator as u128)
            > (baseline_b as u128) * (damped_total as u128)
    );
}

#[test]
fn fully_depleted_collection_is_dropped_not_zeroed() {
    let mut inputs = vec![input("a-in", "a", 2, dec!(0.20)); 5];
    inputs.extend(vec![input("b-in", "b", 2, dec!(0.20)); 5]);
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("b-1", "b", 3, true)],
    )
    .unwrap();

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a".to_string(),
        ScarcityMultiplier {
            numerator: 0,
            denominator: 10,
        },
    );
    let damped = apply_collection_scarcity(&outcomes, &multipliers).unwrap();

    assert_eq!(damped.len(), 1);
    assert_eq!(damped[0].collection_id, "b");
    assert!(damped.iter().all(|o| o.weight_numerator > 0));
}

#[test]
fn all_collections_depleted_is_rejected() {
    let inputs = vec![input("in", "a", 2, dec!(0.20)); 10];
    let outcomes = build_outcomes(&inputs, &[output("a-1", "a", 3, true)]).unwrap();

    let mut multipliers = BTreeMap::new();
    multipliers.insert(
        "a".to_string(),
        ScarcityMultiplier {
            numerator: 0,
            denominator: 1,
        },
    );
    let error = apply_collection_scarcity(&outcomes, &multipliers).unwrap_err();
    assert_eq!(error, TradeupError::InvalidWeights);
}

#[test]
fn draining_stock_cannot_increase_a_collections_own_weight() {
    let mut inputs = vec![input("a-in", "a", 2, dec!(0.20)); 6];
    inputs.extend(vec![input("b-in", "b", 2, dec!(0.20)); 4]);
    let outcomes = build_outcomes(
        &inputs,
        &[output("a-1", "a", 3, true), output("b-1", "b", 3, true)],
    )
    .unwrap();

    let well_stocked = apply_collection_scarcity(
        &outcomes,
        &BTreeMap::from([(
            "a".to_string(),
            ScarcityMultiplier {
                numerator: 8,
                denominator: 10,
            },
        )]),
    )
    .unwrap();
    let depleted = apply_collection_scarcity(
        &outcomes,
        &BTreeMap::from([(
            "a".to_string(),
            ScarcityMultiplier {
                numerator: 3,
                denominator: 10,
            },
        )]),
    )
    .unwrap();

    let collection_share = |set: &[economy_core::tradeup::WeightedOutcome]| -> (u128, u128) {
        let numerator: u128 = set
            .iter()
            .filter(|o| o.collection_id == "a")
            .map(|o| o.weight_numerator as u128)
            .sum();
        (numerator, set[0].weight_denominator as u128)
    };
    let (well_numerator, well_denominator) = collection_share(&well_stocked);
    let (depleted_numerator, depleted_denominator) = collection_share(&depleted);

    // depleted share <= well-stocked share (cross-multiplied to avoid floats).
    assert!(depleted_numerator * well_denominator <= well_numerator * depleted_denominator);
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
