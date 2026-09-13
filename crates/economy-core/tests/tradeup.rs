use economy_core::tradeup::{
    InputItem, OutputItem, TradeupError, build_outcomes, calculate_output_float, select_outcome,
    server_seed_commitment,
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
