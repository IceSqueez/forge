use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::stream as stream_fields;

pub(crate) struct StreamOfflineDescriptor;

impl TriggerKindDescriptor for StreamOfflineDescriptor {
    fn id(&self) -> &str {
        "twitch.stream.offline"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Stream offline"
    }

    fn summary(&self) -> &str {
        "Fires when your stream ends"
    }

    fn search_text(&self) -> &str {
        "twitch stream offline ended stopped broadcast"
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
            kind_prefix: Some("twitch.stream.offline".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
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

    fn offline_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "stream.offline",
            serde_json::json!({
                "broadcaster": { "id": "100", "login": "host_chan" }
            }),
        )
    }

    #[test]
    fn event_filter_gates_on_stream_offline_kind_prefix() {
        let filter = StreamOfflineDescriptor.event_filter();
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.stream.offline"));
        assert_eq!(filter.source, Some(EventSource::Twitch));
    }

    #[test]
    fn build_arg_stack_maps_broadcaster_from_nested_payload() {
        let stack = StreamOfflineDescriptor.build_arg_stack(&offline_event());
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
