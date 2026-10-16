use forge_registry::{SubActionRegistry, nested_chains};
use forge_types::{SubActionConfig, SubActionStep, Variant};

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
