use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::hype_train as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("hype.id", "Hype Train ID", fields::HYPE, fields::HYPE_ID),
    Variable::nested_int(
        "hype.level",
        "Hype Train level",
        fields::HYPE,
        fields::LEVEL,
    )
    .synthesis(SynthesisHint::BoundedInt {
        min: 1,
        max: super::HYPE_TRAIN_LEVEL_MAX,
    }),
    Variable::nested_int("hype.goal", "Level goal", fields::HYPE, fields::GOAL).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_int(
        "hype.progress",
        "Current progress",
        fields::HYPE,
        fields::PROGRESS,
    )
    .synthesis(SynthesisHint::BoundedInt {
        min: 0,
        max: super::MAX_SYNTHESIZED_COUNT,
    }),
    Variable::nested_int("hype.total", "Total points", fields::HYPE, fields::TOTAL).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_timestamp(
        "hype.started_at",
        "Started at",
        fields::HYPE,
        fields::STARTED_AT,
    ),
    Variable::nested_timestamp(
        "hype.expires_at",
        "Expires at",
        fields::HYPE,
        fields::EXPIRES_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_HYPE_TRAIN_STARTED,
    category: TriggerCategory::Hype,
    label: "Hype Train started",
    summary: "Fires when a Hype Train begins on the channel",
    search_text: "twitch hype train begin start level",
    icon: "train",
    kind: event_kinds::CHANNEL_HYPE_TRAIN_BEGIN,
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

    fn begin_event() -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-1",
                "level": 3,
                "goal": 1000,
                "progress": 450,
                "total": 450,
                "started_at": "2026-06-13T18:00:00Z",
                "expires_at": "2026-06-13T18:05:00Z",
            }
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.hype_train.begin",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_hype_train_begin_on_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.hype_train.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_progress_fields() {
        let stack = SPEC.build_arg_stack(&begin_event());
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-1".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(3)));
        assert_eq!(stack.get("hype.goal"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("hype.progress"), Some(&Variant::Int(450)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(450)));
        assert_eq!(
            stack.get("hype.started_at"),
            Some(&instant("2026-06-13T18:00:00Z"))
        );
        assert_eq!(
            stack.get("hype.expires_at"),
            Some(&instant("2026-06-13T18:05:00Z"))
        );
    }
}
