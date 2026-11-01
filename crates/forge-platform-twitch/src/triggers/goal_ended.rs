use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

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
    .synthesis(SynthesisHint::BoundedInt {
        min: 0,
        max: super::MAX_SYNTHESIZED_COUNT,
    }),
    Variable::nested_int(
        "goal.target_amount",
        "Target amount",
        fields::GOAL,
        fields::TARGET_AMOUNT,
    )
    .synthesis(SynthesisHint::BoundedInt {
        min: 0,
        max: super::MAX_SYNTHESIZED_COUNT,
    }),
    Variable::nested_flag(
        "goal.is_achieved",
        "Goal achieved",
        fields::GOAL,
        fields::IS_ACHIEVED,
    ),
    Variable::nested_timestamp("goal.ended_at", "Ended at", fields::GOAL, fields::ENDED_AT),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GOAL_ENDED,
    category: TriggerCategory::Goals,
    label: "Goal ended",
    summary: "Fires when a creator goal ends, whether achieved or not",
    search_text: "twitch goal ended completed achieved follower subscription",
    icon: "flag",
    kind: event_kinds::CHANNEL_GOAL_END,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn goal_end_event(is_achieved: bool) -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-3",
                "type": "follower",
                "current_amount": 1000,
                "target_amount": 1000,
                "is_achieved": is_achieved,
                "ended_at": "2026-06-13T19:00:00Z",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.goal.end", payload)
    }

    #[test]
    fn event_filter_targets_goal_end_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.goal.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_amounts_as_int_and_is_achieved_as_bool() {
        let stack = SPEC.build_arg_stack(&goal_end_event(true));
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-3".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("goal.is_achieved"), Some(&Variant::Bool(true)));
        assert_eq!(
            stack.get("goal.ended_at"),
            Some(&instant("2026-06-13T19:00:00Z"))
        );
    }

    #[test]
    fn build_arg_stack_preserves_unachieved_goal_as_bool_false() {
        let stack = SPEC.build_arg_stack(&goal_end_event(false));
        assert_eq!(stack.get("goal.is_achieved"), Some(&Variant::Bool(false)));
    }
}
