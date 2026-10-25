use forge_registry::SubActionRegistry;
use forge_types::SubActionStep;

pub(super) fn sends_order_sensitive_overlay(
    steps: &[SubActionStep],
    registry: &SubActionRegistry,
    order_sensitive: &dyn Fn(&str) -> bool,
) -> bool {
    forge_runtime::overlay_send_targets(steps, registry)
        .iter()
        .filter_map(forge_runtime::OverlaySendTarget::overlay)
        .any(order_sensitive)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::sub_action_runners::CoreLogicIfThenElseRunner;
    use forge_runtime::{ConditionGate, Config};
    use forge_types::{SubActionConfig, Variant};

    use super::*;
    use crate::actions_screen::nav;

    const BRANCH_KIND: &str = "core.logic.if_then_else";
    const THEN_CHAIN_KEY: &str = "then_chain";
    const ORDERED: &str = "chat-wall";
    const UNORDERED: &str = "alert-box";

    fn registry() -> SubActionRegistry {
        let gate = Arc::new(ConditionGate::new(&Config::default()));
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(CoreLogicIfThenElseRunner::new(gate)))
            .expect("the branching runner registers");
        reg
    }

    fn step(kind_id: &str, config: SubActionConfig, enabled: bool) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn send(identity: &str) -> SubActionStep {
        step(
            forge_runtime::OVERLAY_SEND_KIND_ID,
            SubActionConfig::from([(
                forge_runtime::OVERLAY_TARGET_KEY.to_owned(),
                Variant::String(identity.to_owned()),
            )]),
            true,
        )
    }

    fn branch(body: Vec<SubActionStep>) -> SubActionStep {
        step(
            BRANCH_KIND,
            SubActionConfig::from([(THEN_CHAIN_KEY.to_owned(), nav::encode_chain(&body))]),
            true,
        )
    }

    #[test]
    fn only_a_resolved_order_sensitive_target_raises_the_warning() {
        for (steps, expected, label) in [
            (
                vec![send(ORDERED)],
                true,
                "a step sending to an overlay whose delivery order matters",
            ),
            (
                vec![send(UNORDERED)],
                false,
                "a step sending to an overlay whose delivery order does not matter",
            ),
            (
                vec![branch(vec![send(ORDERED)])],
                true,
                "an ordered overlay inside a branch this screen encoded itself",
            ),
            (
                vec![send("%overlay_target%")],
                false,
                "an overlay named by a variable no stored identity matches",
            ),
        ] {
            assert_eq!(
                sends_order_sensitive_overlay(&steps, &registry(), &|id| id == ORDERED),
                expected,
                "{label}"
            );
        }
    }
}
