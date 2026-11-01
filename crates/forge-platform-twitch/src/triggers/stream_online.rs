use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::stream as stream_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text(
        "stream_id",
        "Stream ID",
        stream_fields::STREAM,
        stream_fields::STREAM_ID,
    ),
    Variable::nested_text(
        "stream_type",
        "Stream type",
        stream_fields::STREAM,
        stream_fields::STREAM_TYPE,
    ),
    Variable::nested_timestamp(
        "started_at",
        "Started at",
        stream_fields::STREAM,
        stream_fields::STARTED_AT,
    ),
    Variable::nested_text(
        "broadcaster_login",
        "Broadcaster login",
        stream_fields::BROADCASTER,
        stream_fields::BROADCASTER_LOGIN,
    )
    .synthesis(SynthesisHint::Username),
    Variable::nested_text(
        "broadcaster_id",
        "Broadcaster ID",
        stream_fields::BROADCASTER,
        stream_fields::BROADCASTER_ID,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::STREAM_ONLINE,
    category: TriggerCategory::Streams,
    label: "Stream online",
    summary: "Fires when your stream goes live",
    search_text: "twitch stream online live started broadcast",
    icon: "broadcast",
    kind: event_kinds::STREAM_ONLINE,
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

    fn online_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "stream.online",
            serde_json::json!({
                "stream": { "id": "stream-1", "type": "live", "started_at": "2026-06-13T09:00:00Z" },
                "broadcaster": { "id": "100", "login": "host_chan" }
            }),
        )
    }

    #[test]
    fn event_filter_gates_on_stream_online_kind_prefix() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.stream.online"));
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_stream_and_broadcaster_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&online_event());
        assert_eq!(
            stack.get("stream_id"),
            Some(&Variant::String("stream-1".to_owned()))
        );
        assert_eq!(
            stack.get("stream_type"),
            Some(&Variant::String("live".to_owned()))
        );
        assert_eq!(
            stack.get("started_at"),
            Some(&instant("2026-06-13T09:00:00Z"))
        );
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String("host_chan".to_owned()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String("100".to_owned()))
        );
    }
}
