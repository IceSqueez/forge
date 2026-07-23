use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::stream as stream_fields;

pub(crate) struct StreamOnlineDescriptor;

impl TriggerKindDescriptor for StreamOnlineDescriptor {
    fn id(&self) -> &str {
        "twitch.stream.online"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Stream online"
    }

    fn summary(&self) -> &str {
        "Fires when your stream goes live"
    }

    fn search_text(&self) -> &str {
        "twitch stream online live started broadcast"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.stream.online".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let stream_id = event
            .payload
            .get(stream_fields::STREAM)
            .and_then(|s| s.get(stream_fields::STREAM_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let stream_type = event
            .payload
            .get(stream_fields::STREAM)
            .and_then(|s| s.get(stream_fields::STREAM_TYPE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let started_at = event
            .payload
            .get(stream_fields::STREAM)
            .and_then(|s| s.get(stream_fields::STARTED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let broadcaster_login = event
            .payload
            .get(stream_fields::BROADCASTER)
            .and_then(|b| b.get(stream_fields::BROADCASTER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let broadcaster_id = event
            .payload
            .get(stream_fields::BROADCASTER)
            .and_then(|b| b.get(stream_fields::BROADCASTER_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("stream_id".to_owned(), Variant::String(stream_id))
            .set("stream_type".to_owned(), Variant::String(stream_type))
            .set("started_at".to_owned(), Variant::String(started_at))
            .set(
                "broadcaster_login".to_owned(),
                Variant::String(broadcaster_login),
            )
            .set("broadcaster_id".to_owned(), Variant::String(broadcaster_id))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "stream_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Stream ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "stream_type".to_owned(),
                        kind: VariantKind::String,
                        label: "Stream type".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "broadcaster_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcaster login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "broadcaster_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcaster ID".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        let filter = StreamOnlineDescriptor.event_filter();
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.stream.online"));
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_stream_and_broadcaster_from_nested_payload() {
        let stack = StreamOnlineDescriptor.build_arg_stack(&online_event());
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
            Some(&Variant::String("2026-06-13T09:00:00Z".to_owned()))
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
