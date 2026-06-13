use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct GuestStarSessionEndedDescriptor;

impl TriggerKindDescriptor for GuestStarSessionEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.session_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star session ended"
    }

    fn summary(&self) -> &str {
        "Fires when a running Guest Star session ends"
    }

    fn search_text(&self) -> &str {
        "twitch guest star session ended stopped host"
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
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.guest_star_session.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let session = event.payload.get("session");

        // guest_star.session_id is the chaining var for guest-star sub-actions (%guest_star.session_id%).
        let session_id = session
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = session
            .and_then(|s| s.get("ended_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "guest_star.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("guest_star.ended_at".to_owned(), Variant::String(ended_at))
    }
}
