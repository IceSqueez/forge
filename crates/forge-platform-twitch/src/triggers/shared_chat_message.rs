use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

use super::chat_arg_stack::base_chat_args;

pub(crate) struct SharedChatMessageDescriptor;

impl TriggerKindDescriptor for SharedChatMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.shared_chat.message_received"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared chat message"
    }

    fn summary(&self) -> &str {
        "Fires when a message arrives via a Shared Chat session from another channel"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session source channel cross-channel"
    }

    fn icon_name(&self) -> &str {
        "messages"
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
        "any shared chat message".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
            .payload
            .get("from_channel")
            .and_then(|fc| fc.get("login"))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let from_login = event
            .payload
            .get("from_channel")
            .and_then(|fc| fc.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_display_name = event
            .payload
            .get("from_channel")
            .and_then(|fc| fc.get("display_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        base_chat_args(event)
            .set(
                "chat.from_channel.login".to_owned(),
                Variant::String(from_login),
            )
            .set(
                "chat.from_channel.display_name".to_owned(),
                Variant::String(from_display_name),
            )
    }
}
