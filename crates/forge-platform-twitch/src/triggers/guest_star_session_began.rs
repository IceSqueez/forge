use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct GuestStarSessionBeganDescriptor;

impl TriggerKindDescriptor for GuestStarSessionBeganDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.session_began"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star session began"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster starts a new Guest Star session"
    }

    fn search_text(&self) -> &str {
        "twitch guest star session began started host"
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
            kind_prefix: Some("channel.guest_star_session.begin".to_owned()),
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
        let started_at = session
            .and_then(|s| s.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "guest_star.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set(
                "guest_star.started_at".to_owned(),
                Variant::String(started_at),
            )
    }
}
