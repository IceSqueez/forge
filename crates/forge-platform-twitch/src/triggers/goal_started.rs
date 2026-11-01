use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::goal as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("goal.id", fields::GOAL, fields::GOAL_ID),
    Variable::nested_text("goal.type", fields::GOAL, fields::GOAL_TYPE),
    Variable::nested_text("goal.description", fields::GOAL, fields::DESCRIPTION),
    Variable::nested_int("goal.current_amount", fields::GOAL, fields::CURRENT_AMOUNT).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_int("goal.target_amount", fields::GOAL, fields::TARGET_AMOUNT).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_timestamp("goal.started_at", fields::GOAL, fields::STARTED_AT),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::GOAL_STARTED,
    category: TriggerCategory::Goals,
    label: "Goal started",
    summary: "Fires when a creator goal begins on the broadcaster's channel",
    search_text: "twitch goal started begin follower subscription",
    icon: "flag",
    kind: event_kinds::CHANNEL_GOAL_BEGIN,
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

    fn goal_begin_event() -> Event {
        let payload = serde_json::json!({
            "goal": {
                "id": "goal-1",
                "type": "follower",
                "description": "Road to 1k",
                "current_amount": 250,
                "target_amount": 1000,
                "started_at": "2026-06-13T18:00:00Z",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.goal.begin", payload)
    }

    #[test]
    fn build_arg_stack_maps_amounts_as_int_and_metadata_as_string() {
        let stack = SPEC.build_arg_stack(&goal_begin_event());
        assert_eq!(
            stack.get("goal.id"),
            Some(&Variant::String("goal-1".to_owned()))
        );
        assert_eq!(
            stack.get("goal.type"),
            Some(&Variant::String("follower".to_owned()))
        );
        assert_eq!(
            stack.get("goal.description"),
            Some(&Variant::String("Road to 1k".to_owned()))
        );
        assert_eq!(stack.get("goal.current_amount"), Some(&Variant::Int(250)));
        assert_eq!(stack.get("goal.target_amount"), Some(&Variant::Int(1000)));
        assert_eq!(
            stack.get("goal.started_at"),
            Some(&instant("2026-06-13T18:00:00Z"))
        );
    }
}
