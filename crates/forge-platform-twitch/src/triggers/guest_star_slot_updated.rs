use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct GuestStarSlotUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarSlotUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.slot_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star slot updated"
    }

    fn summary(&self) -> &str {
        "Fires when a Guest Star slot changes (host video/audio toggle, volume)"
    }

    fn search_text(&self) -> &str {
        "twitch guest star slot update host video audio volume"
    }

    fn icon_name(&self) -> &str {
        "star"
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
        "any slot".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.guest_star_slot.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let slot = event.payload.get("slot");

        let session_id = event
            .payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let slot_id = slot
            .and_then(|s| s.get("slot_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let host_video_enabled = slot
            .and_then(|s| s.get("host_video_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let host_audio_enabled = slot
            .and_then(|s| s.get("host_audio_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let volume = slot
            .and_then(|s| s.get("volume"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("session.id".to_owned(), Variant::String(session_id))
            .set("slot.id".to_owned(), Variant::String(slot_id))
            .set(
                "slot.host_video_enabled".to_owned(),
                Variant::Bool(host_video_enabled),
            )
            .set(
                "slot.host_audio_enabled".to_owned(),
                Variant::Bool(host_audio_enabled),
            )
            .set("slot.volume".to_owned(), Variant::Int(volume))
    }
}
