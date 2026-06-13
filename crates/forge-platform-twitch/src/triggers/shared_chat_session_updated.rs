use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct SharedChatSessionUpdatedDescriptor;

impl TriggerKindDescriptor for SharedChatSessionUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.shared_chat.session_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared Chat session updated"
    }

    fn summary(&self) -> &str {
        "Fires when the active shared chat session the broadcaster's channel is in changes"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session updated changed host"
    }

    fn icon_name(&self) -> &str {
        "chat"
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
            kind_prefix: Some("channel.shared_chat.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let shared_chat = event.payload.get("shared_chat");
        let host = event.payload.get("host");

        let session_id = shared_chat
            .and_then(|s| s.get("session_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let host_login = host
            .and_then(|h| h.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "shared_chat.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("host_login".to_owned(), Variant::String(host_login))
    }
}
