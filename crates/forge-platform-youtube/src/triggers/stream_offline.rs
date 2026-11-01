use forge_events::Event;
use forge_events::EventSource;
use forge_registry::{
    ActorDeclaration, EventFilter, KindPlatformContract, TriggerCatalogEntry, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read;
use crate::vocabulary::fields::stream as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ChannelBroadcastEndedDescriptor;

impl TriggerCatalogEntry for ChannelBroadcastEndedDescriptor {
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
}

impl TriggerKindDescriptor for ChannelBroadcastEndedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::STREAM_OFFLINE
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::STREAM_OFFLINE.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(TriggerVariables::new().event_specific(
            DeclaredVariable {
                name: variables::stream::BROADCAST_ID.to_owned(),
                kind: VariantKind::String,
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
