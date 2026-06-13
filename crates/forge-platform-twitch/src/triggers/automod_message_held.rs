use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AutomodMessageHeldDescriptor;

impl TriggerKindDescriptor for AutomodMessageHeldDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.message_held"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod message held"
    }

    fn summary(&self) -> &str {
        "Fires when AutoMod holds a chat message for moderator review"
    }

    fn search_text(&self) -> &str {
        "twitch automod message held review moderation"
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
            kind_prefix: Some("channel.automod.message.hold".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let automod = event.payload.get("automod");
        let user = event.payload.get("user");

        // automod.message_id is the key input for approve_message/deny_message sub-actions.
        let message_id = automod
            .and_then(|a| a.get("message_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category = automod
            .and_then(|a| a.get("category"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let level = automod
            .and_then(|a| a.get("level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let held_at = automod
            .and_then(|a| a.get("held_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_id = user
            .and_then(|u| u.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let message_text = event
            .payload
            .get("message_text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("automod.message_id".to_owned(), Variant::String(message_id))
            .set("automod.category".to_owned(), Variant::String(category))
            .set("automod.level".to_owned(), Variant::Int(level))
            .set("held_at".to_owned(), Variant::String(held_at))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("user_id".to_owned(), Variant::String(user_id))
            .set("message_text".to_owned(), Variant::String(message_text))
    }
}
