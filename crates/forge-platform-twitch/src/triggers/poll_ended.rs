use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::VariantKind;

use super::families::build_choices_variant;
use crate::vocabulary::fields::poll as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("poll.id", fields::POLL, fields::POLL_ID),
    Variable::nested_text("poll.title", fields::POLL, fields::POLL_TITLE),
    Variable::nested_text("poll.status", fields::POLL, fields::STATUS),
    Variable::nested_timestamp("poll.ended_at", fields::POLL, fields::ENDED_AT),
    Variable::custom("poll.choices", VariantKind::Array, build_choices_variant),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::POLL_ENDED,
    category: TriggerCategory::Polls,
    label: "Poll ended",
    summary: "Fires when a poll ends (completed, archived, or terminated)",
    search_text: "twitch poll ended completed archived terminated",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_POLL_END,
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

    fn poll_end_event() -> Event {
        let payload = serde_json::json!({
            "poll": {
                "id": "poll-3",
                "title": "Map vote",
                "status": "completed",
                "ended_at": "2026-06-13T18:10:00Z",
            },
        });
        Event::new(EventSource::Twitch, "twitch.channel.poll.end", payload)
    }

    #[test]
    fn event_filter_targets_poll_end_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.poll.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_status_and_ended_at_alongside_id_and_title() {
        let stack = SPEC.build_arg_stack(&poll_end_event());
        assert_eq!(
            stack.get("poll.id"),
            Some(&Variant::String("poll-3".to_owned()))
        );
        assert_eq!(
            stack.get("poll.title"),
            Some(&Variant::String("Map vote".to_owned()))
        );
        assert_eq!(
            stack.get("poll.status"),
            Some(&Variant::String("completed".to_owned()))
        );
        assert_eq!(
            stack.get("poll.ended_at"),
            Some(&instant("2026-06-13T18:10:00Z"))
        );
    }
}
