use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChatMessageDeletedDescriptor;

impl TriggerKindDescriptor for ChatMessageDeletedDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.message_deleted"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Message deleted"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deletes a message from YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube chat message deleted removed moderation moderator"
    }

    fn icon_name(&self) -> &str {
        "trash-2"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.chat.message_deleted".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_id = event
            .payload
            .get("chat.message_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let target_channel_id = event
            .payload
            .get("chat.target_user.channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_channel_id = event
            .payload
            .get("chat.moderator.channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("chat.message_id".to_owned(), Variant::String(message_id))
            .set(
                "chat.target_user.channel_id".to_owned(),
                Variant::String(target_channel_id),
            )
            .set(
                "chat.moderator.channel_id".to_owned(),
                Variant::String(moderator_channel_id),
            )
    }
}
