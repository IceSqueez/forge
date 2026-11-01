use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::hype_train as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("hype.id", fields::HYPE, fields::HYPE_ID),
    Variable::nested_int("hype.level", fields::HYPE, fields::LEVEL).synthesis(
        SynthesisHint::BoundedInt {
            min: 1,
            max: super::HYPE_TRAIN_LEVEL_MAX,
        },
    ),
    Variable::nested_int("hype.total", fields::HYPE, fields::TOTAL).synthesis(
        SynthesisHint::BoundedInt {
            min: 0,
            max: super::MAX_SYNTHESIZED_COUNT,
        },
    ),
    Variable::nested_timestamp("hype.ended_at", fields::HYPE, fields::ENDED_AT),
    Variable::nested_timestamp(
        "hype.cooldown_ends_at",
        fields::HYPE,
        fields::COOLDOWN_ENDS_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SUPPORT_HYPE_TRAIN_ENDED,
    category: TriggerCategory::Hype,
    label: "Hype Train ended",
    summary: "Fires when a Hype Train concludes on the channel",
    search_text: "twitch hype train end finish level cooldown",
    icon: "train",
    kind: event_kinds::CHANNEL_HYPE_TRAIN_END,
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

    fn end_event() -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-9",
                "level": 5,
                "total": 9001,
                "ended_at": "2026-06-13T18:10:00Z",
                "cooldown_ends_at": "2026-06-13T19:10:00Z",
            }
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.hype_train.end",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_hype_train_end_on_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.hype_train.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_level_and_total() {
        let stack = SPEC.build_arg_stack(&end_event());
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-9".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(5)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(9001)));
        assert_eq!(
            stack.get("hype.ended_at"),
            Some(&instant("2026-06-13T18:10:00Z"))
        );
        assert_eq!(
            stack.get("hype.cooldown_ends_at"),
            Some(&instant("2026-06-13T19:10:00Z"))
        );
    }
}
