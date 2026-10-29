use forge_registry::{StepNode, StepReach, SubActionRegistry, walk_steps};
use forge_types::{SubActionConfig, SubActionStep, Variant, variable_references};

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
    walk_steps(steps, registry, StepReach::Enabled, &mut |node| {
        if let StepNode::Step(step) = node
            && step.kind_id == OVERLAY_SEND_KIND_ID
        {
            out.extend(target_of(&step.config));
        }
    });
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
    variable_references(target).next().is_some()
}
