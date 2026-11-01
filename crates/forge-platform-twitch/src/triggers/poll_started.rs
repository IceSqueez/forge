use super::families::build_choices_variant;
use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::VariantKind;

use crate::vocabulary::fields::poll as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("poll.id", "Poll ID", fields::POLL, fields::POLL_ID),
    Variable::nested_text("poll.title", "Poll title", fields::POLL, fields::POLL_TITLE),
    Variable::nested_timestamp(
        "poll.started_at",
        "Started at",
        fields::POLL,
        fields::STARTED_AT,
    ),
    Variable::nested_timestamp("poll.ends_at", "Ends at", fields::POLL, fields::ENDS_AT),
    Variable::custom(
        "poll.choices",
        VariantKind::Array,
        "Poll choices",
        build_choices_variant,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::POLL_STARTED,
    category: TriggerCategory::Polls,
    label: "Poll started",
    summary: "Fires when a poll begins on the broadcaster's channel",
    search_text: "twitch poll started begin vote",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_POLL_BEGIN,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::Event;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn poll_begin_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-1",
                "title": "Best emote?",
                "started_at": "2026-06-13T18:00:00Z",
                "ends_at": "2026-06-13T18:05:00Z",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.poll.begin", payload)
    }

    #[test]
    fn event_filter_targets_poll_begin_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.poll.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_poll_id_title_and_timing_fields() {
        let stack = SPEC.build_arg_stack(&poll_begin_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-1".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Best emote?".to_owned()))
        );
        assert_eq!(
            stack.get("poll.started_at"),
            Some(&instant("2026-06-13T18:00:00Z"))
        );
        assert_eq!(
            stack.get("poll.ends_at"),
            Some(&instant("2026-06-13T18:05:00Z"))
        );
    }
}
