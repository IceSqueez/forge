use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::goal as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("goal.id", "Goal ID", fields::GOAL, fields::GOAL_ID),
    Variable::nested_text("goal.type", "Goal type", fields::GOAL, fields::GOAL_TYPE),
    Variable::nested_int(
        "goal.current_amount",
        "Current amount",
        fields::GOAL,
        fields::CURRENT_AMOUNT,
    )
    .bounded(0, super::MAX_SYNTHESIZED_COUNT),
    Variable::nested_int(
        "goal.target_amount",
        "Target amount",
        fields::GOAL,
        fields::TARGET_AMOUNT,
    )
    .bounded(0, super::MAX_SYNTHESIZED_COUNT),
    Variable::nested_flag(
        "goal.is_synced",
        "Synced on connect",
        fields::GOAL,
        fields::IS_SYNCED,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GOAL_PROGRESS,
    category: TriggerCategory::Goals,
    label: "Goal progress",
    summary: "Fires when a creator goal receives a progress update",
    search_text: "twitch goal progress update follower subscription",
    icon: "flag",
    kind: event_kinds::CHANNEL_GOAL_PROGRESS,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::{Variant, VariantKind};

    fn goal_progress_event() -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-2",
                "type": "subscription",
                "current_amount": 42,
                "target_amount": 100,
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.goal.progress", payload)
    }

    #[test]
    fn build_arg_stack_maps_progress_amounts_as_int() {
        let stack = SPEC.build_arg_stack(&goal_progress_event());
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-2".to_owned()))
        );
        assert_eq!(
            stack.get("goal.type"),
            Some(&Variant::String("subscription".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(42)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(100)));
    }

    #[test]
    fn build_arg_stack_reads_is_synced_and_treats_absent_or_non_bool_as_live() {
        for (flag, expected) in [
            (Some(serde_json::json!(true)), true),
            (Some(serde_json::json!(false)), false),
            (Some(serde_json::json!("true")), false),
            (None, false),
        ] {
            let mut event = goal_progress_event();
            if let Some(flag) = flag.clone() {
                event.payload["goal"]["is_synced"] = flag;
            }

            let stack = SPEC.build_arg_stack(&event);

            assert_eq!(
                stack.get("goal.is_synced"),
                Some(&Variant::Bool(expected)),
                "flag {flag:?}"
            );
        }
    }

    fn kind_of(value: &Variant) -> VariantKind {
        match value {
            Variant::Int(_) => VariantKind::Int,
            Variant::Float(_) => VariantKind::Float,
            Variant::Bool(_) => VariantKind::Bool,
            Variant::String(_) => VariantKind::String,
            Variant::Datetime(_) => VariantKind::Datetime,
            Variant::Array(_) => VariantKind::Array,
            Variant::Object(_) => VariantKind::Object,
        }
    }

    #[test]
    fn output_schema_declares_every_variable_the_arg_stack_sets_with_its_kind() {
        let produced: Vec<(String, VariantKind)> = SPEC
            .build_arg_stack(&goal_progress_event())
            .snapshot()
            .iter()
            .map(|(name, value)| (name.clone(), kind_of(value)))
            .collect();

        let mut declared: Vec<(String, VariantKind)> = SPEC
            .output_schema()
            .unwrap()
            .variables
            .into_iter()
            .map(|variable| (variable.name, variable.kind))
            .collect();
        declared.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(declared, produced);
    }
}
