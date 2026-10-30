use forge_registry::decode_steps;
use forge_types::{SubActionStep, Variant};

const STORED_CHAINS: &str = include_str!("fixtures/step_tree/nested_chains.json");
const DECODED_GOLDEN: &str = include_str!("fixtures/step_tree/nested_chains.decoded.json");

#[test]
fn stored_nested_chains_decode_to_the_golden_steps_with_todays_flag_defaults() {
    let chains: Vec<Variant> = serde_json::from_str(STORED_CHAINS).expect("variant chains");

    let decoded: Vec<Vec<SubActionStep>> = chains
        .iter()
        .map(|chain| decode_steps(Some(chain)))
        .collect();

    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&decoded).unwrap()),
        DECODED_GOLDEN
    );
}
