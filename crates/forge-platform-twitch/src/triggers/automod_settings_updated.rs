use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AutomodSettingsUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodSettingsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.settings_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod settings updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator changes the channel AutoMod filter settings"
    }

    fn search_text(&self) -> &str {
        "twitch automod settings filter level moderator updated changed"
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
            kind_prefix: Some("channel.automod.settings.update".to_owned()),
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
        let overall_level = event
            .payload
            .get("overall_level")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("moderator_id".to_owned(), Variant::String(moderator_id))
            .set(
                "automod.overall_level".to_owned(),
                Variant::Int(overall_level),
            )
    }
}
