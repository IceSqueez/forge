use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::payload_fields::stream as fields;

pub(crate) struct ChannelBroadcastEndedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastEndedDescriptor {
    fn id(&self) -> &str {
        "youtube.stream.offline"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Live broadcast ended"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube live broadcast ends"
    }

    fn search_text(&self) -> &str {
        "youtube live stream offline broadcast ended channel"
    }

    fn icon_name(&self) -> &str {
        "radio-off"
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
            kind_prefix: Some("youtube.stream.offline".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(TriggerVariables::new().event_specific(
            DeclaredVariable {
                name: "broadcast_id".to_owned(),
                kind: VariantKind::String,
                label: "Broadcast ID".to_owned(),
                synthesis: None,
            },
            |event| Variant::String(payload_read::text(event, fields::BROADCAST_ID)),
        ))
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn broadcast_ended_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.stream.offline",
            serde_json::json!({
                "broadcast_id": "broadcast_xyz"
            }),
        )
    }

    #[test]
    fn a_broadcast_ending_names_the_broadcast_that_ended() {
        let stack = ChannelBroadcastEndedDescriptor.build_arg_stack(&broadcast_ended_event());
        assert_eq!(
            stack.get("broadcast_id"),
            Some(&Variant::String("broadcast_xyz".to_owned()))
        );
    }
}
