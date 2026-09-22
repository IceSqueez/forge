use forge_registry::{FormField, SubActionRegistry};
use forge_types::{SubActionConfig, SubActionStep, Variant};

use super::core_logic_shared::{CASE_CHAIN_KEY, decode_steps};

pub const OVERLAY_SEND_KIND_ID: &str = "overlay.send";
pub const OVERLAY_TARGET_KEY: &str = "overlay_id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlaySendTarget {
    Overlay(String),
    Unresolved(String),
}

impl OverlaySendTarget {
    pub fn overlay(&self) -> Option<&str> {
        match self {
            Self::Overlay(identity) => Some(identity),
            Self::Unresolved(_) => None,
        }
    }
}

pub fn overlay_send_targets(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
) -> Vec<OverlaySendTarget> {
    let mut targets = Vec::new();
    collect_targets(steps, registry, &mut targets);
    targets
}

pub fn feeds_overlay(steps: &[SubActionStep], registry: &SubActionRegistry, overlay: &str) -> bool {
    overlay_send_targets(steps, registry)
        .iter()
        .filter_map(OverlaySendTarget::overlay)
        .any(|identity| identity == overlay)
}

fn collect_targets(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    out: &mut Vec<OverlaySendTarget>,
) {
    for step in steps.iter().filter(|step| step.enabled) {
        if step.kind_id == OVERLAY_SEND_KIND_ID {
            out.extend(target_of(&step.config));
        }
        for chain in nested_chains(step, registry) {
            collect_targets(&chain, registry, out);
        }
    }
}

fn target_of(config: &SubActionConfig) -> Option<OverlaySendTarget> {
    let target = config
        .get(OVERLAY_TARGET_KEY)
        .and_then(Variant::as_str)?
        .trim();
    if target.is_empty() {
        return None;
    }
    if holds_variable_token(target) {
        Some(OverlaySendTarget::Unresolved(target.to_owned()))
    } else {
        Some(OverlaySendTarget::Overlay(target.to_owned()))
    }
}

fn holds_variable_token(target: &str) -> bool {
    let mut rest = target;
    while let Some((_, opened)) = rest.split_once('%') {
        let Some((name, tail)) = opened.split_once('%') else {
            return false;
        };
        if !name.trim().is_empty() {
            return true;
        }
        rest = tail;
    }
    false
}

fn nested_chains(step: &SubActionStep, registry: &SubActionRegistry) -> Vec<Vec<SubActionStep>> {
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
