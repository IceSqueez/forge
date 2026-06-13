use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ShieldModeStartedDescriptor;

impl TriggerKindDescriptor for ShieldModeStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shield_mode_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shield mode started"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator activates shield mode in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch shield mode protection moderation started activated"
    }

    fn icon_name(&self) -> &str {
        "shield"
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
            kind_prefix: Some("channel.shield_mode.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let moderator = event.payload.get("moderator");

        let moderator_login = moderator
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_id = moderator
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let started_at = event
            .payload
            .get("started_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("moderator_id".to_owned(), Variant::String(moderator_id))
            .set("started_at".to_owned(), Variant::String(started_at))
    }
}
