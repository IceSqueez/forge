use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub(crate) struct ChannelBroadcastStartedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastStartedDescriptor {
    fn id(&self) -> &str {
        "youtube.stream.online"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Live broadcast started"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube live broadcast becomes active"
    }

    fn search_text(&self) -> &str {
        "youtube live stream online broadcast started channel"
    }

    fn icon_name(&self) -> &str {
        "radio"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.stream.online".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let broadcast_title = event
            .payload
            .get("broadcast_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let broadcast_id = event
            .payload
            .get("broadcast_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "broadcast_title".to_owned(),
                Variant::String(broadcast_title),
            )
            .set("broadcast_id".to_owned(), Variant::String(broadcast_id))
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "broadcast_title".to_owned(),
                    kind: VariantKind::String,
                    label: "Broadcast title".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "broadcast_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Broadcast ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn broadcast_started_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.stream.online",
            serde_json::json!({
                "broadcast_title": "Sunday Stream",
                "broadcast_id": "broadcast_xyz"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            ChannelBroadcastStartedDescriptor
                .matches_trigger(&TriggerConfig::new(), &broadcast_started_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_broadcast_fields() {
        let stack = ChannelBroadcastStartedDescriptor.build_arg_stack(&broadcast_started_event());
        assert_eq!(
            stack.get("broadcast_title"),
            Some(&Variant::String("Sunday Stream".to_owned()))
        );
        assert_eq!(
            stack.get("broadcast_id"),
            Some(&Variant::String("broadcast_xyz".to_owned()))
        );
    }
}
