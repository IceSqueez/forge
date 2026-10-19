use forge_types::{SubActionConfig, SubActionStep, Variant};

use crate::form::FormField;
use crate::registry::SubActionRegistry;

pub const CASE_CHAIN_KEY: &str = "chain";
pub const STEP_KIND_KEY: &str = "kind_id";
pub const STEP_CONFIG_KEY: &str = "config";

pub fn decode_steps(value: Option<&Variant>) -> Vec<SubActionStep> {
    let Some(steps) = value.and_then(Variant::as_array) else {
        return Vec::new();
    };
    steps
        .iter()
        .filter_map(|step| {
            let obj = step.as_object()?;
            let kind_id = obj.get(STEP_KIND_KEY).and_then(Variant::as_str)?.to_owned();
            let config = match obj.get(STEP_CONFIG_KEY) {
                Some(Variant::Object(map)) => map.clone(),
                _ => SubActionConfig::new(),
            };
            let enabled = obj
                .get("enabled")
                .and_then(Variant::as_bool)
                .unwrap_or(true);
            let continue_on_error = obj
                .get("continue_on_error")
                .and_then(Variant::as_bool)
                .unwrap_or(false);
            let condition = obj
                .get("condition")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            let label = obj
                .get("label")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            Some(SubActionStep {
                kind_id,
                config,
                enabled,
                continue_on_error,
                condition,
                label,
            })
        })
        .collect()
}

pub fn nested_chains(
    step: &SubActionStep,
    registry: &SubActionRegistry,
) -> Vec<Vec<SubActionStep>> {
    let Some(runner) = registry.get(&step.kind_id) else {
        return Vec::new();
    };
    let mut chains = Vec::new();
    for field in runner.config_fields() {
        match field {
            FormField::SubChain { key, .. } => {
                chains.push(decode_steps(step.config.get(key)));
            }
            FormField::CaseList { key, .. } => {
                let cases = step.config.get(key).and_then(Variant::as_array);
                for case in cases.into_iter().flatten() {
                    chains.push(decode_steps(
                        case.as_object().and_then(|case| case.get(CASE_CHAIN_KEY)),
                    ));
                }
            }
            _ => {}
        }
    }
    chains
}
