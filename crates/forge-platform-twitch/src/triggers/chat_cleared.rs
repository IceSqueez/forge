use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatClearedDescriptor;

impl TriggerKindDescriptor for ChatClearedDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.cleared"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat Cleared"
    }

    fn summary(&self) -> &str {
        "Fires when all messages are cleared from the chat room"
    }

    fn search_text(&self) -> &str {
        "twitch chat clear purge moderation all messages"
    }

    fn icon_name(&self) -> &str {
        "eraser"
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
            kind_prefix: Some("channel.chat.clear".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let broadcaster_login = event
            .payload
            .get("broadcaster")
            .and_then(|b| b.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let broadcaster_id = event
            .payload
            .get("broadcaster")
            .and_then(|b| b.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "broadcaster_login".to_owned(),
                Variant::String(broadcaster_login),
            )
            .set("broadcaster_id".to_owned(), Variant::String(broadcaster_id))
    }
}
