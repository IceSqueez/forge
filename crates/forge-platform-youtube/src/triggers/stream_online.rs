use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::payload_fields::stream as fields;

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

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .event_specific(
                    DeclaredVariable {
                        name: "broadcast_title".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcast title".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::BROADCAST_TITLE)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "broadcast_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Broadcast ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::BROADCAST_ID)),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
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
