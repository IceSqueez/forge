use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct GuestStarSettingsUpdatedDescriptor;

impl TriggerKindDescriptor for GuestStarSettingsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.guest_star.settings_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Guest Star settings updated"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster's Guest Star preferences change"
    }

    fn search_text(&self) -> &str {
        "twitch guest star settings preferences layout slots moderator audio"
    }

    fn icon_name(&self) -> &str {
        "settings"
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
            kind_prefix: Some("channel.guest_star_settings.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let settings = event.payload.get("settings");

        let slot_count = settings
            .and_then(|s| s.get("slot_count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let group_layout = settings
            .and_then(|s| s.get("group_layout"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let is_moderator_send_live_enabled = settings
            .and_then(|s| s.get("is_moderator_send_live_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        ArgStack::new()
            .set("guest_star.slot_count".to_owned(), Variant::Int(slot_count))
            .set(
                "guest_star.group_layout".to_owned(),
                Variant::String(group_layout),
            )
            .set(
                "guest_star.is_moderator_send_live_enabled".to_owned(),
                Variant::Bool(is_moderator_send_live_enabled),
            )
    }
}
