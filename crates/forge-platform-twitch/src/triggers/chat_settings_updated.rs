use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatSettingsUpdatedDescriptor;

impl TriggerKindDescriptor for ChatSettingsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.chat_settings_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat settings updated"
    }

    fn summary(&self) -> &str {
        "Fires when a broadcaster's chat settings change (emote-only, follower-only, slow mode, sub-only, unique-chat)"
    }

    fn search_text(&self) -> &str {
        "twitch chat settings emote follower slow subscriber unique mode updated"
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
            kind_prefix: Some("channel.chat_settings.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let settings = event.payload.get("settings");

        let emote_mode = settings
            .and_then(|s| s.get("emote_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let follower_mode = settings
            .and_then(|s| s.get("follower_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let slow_mode = settings
            .and_then(|s| s.get("slow_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let subscriber_mode = settings
            .and_then(|s| s.get("subscriber_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let unique_chat_mode = settings
            .and_then(|s| s.get("unique_chat_mode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let slow_mode_wait_time_seconds = settings
            .and_then(|s| s.get("slow_mode_wait_time_seconds"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let follower_mode_duration_minutes = settings
            .and_then(|s| s.get("follower_mode_duration_minutes"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set("settings.emote_mode".to_owned(), Variant::Bool(emote_mode))
            .set(
                "settings.follower_mode".to_owned(),
                Variant::Bool(follower_mode),
            )
            .set("settings.slow_mode".to_owned(), Variant::Bool(slow_mode))
            .set(
                "settings.subscriber_mode".to_owned(),
                Variant::Bool(subscriber_mode),
            )
            .set(
                "settings.unique_chat_mode".to_owned(),
                Variant::Bool(unique_chat_mode),
            )
            .set(
                "settings.slow_mode_wait_time_seconds".to_owned(),
                Variant::Int(slow_mode_wait_time_seconds),
            )
            .set(
                "settings.follower_mode_duration_minutes".to_owned(),
                Variant::Int(follower_mode_duration_minutes),
            )
    }
}
