use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct MessageDeletedDescriptor;

impl TriggerKindDescriptor for MessageDeletedDescriptor {
    fn id(&self) -> &str {
        "kick.chat.message_deleted"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Message deleted"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deletes a chat message in Kick"
    }

    fn search_text(&self) -> &str {
        "kick message deleted removed moderated chat"
    }

    fn icon_name(&self) -> &str {
        "trash"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.chat.message_deleted".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let message_id = event
            .payload
            .get("message")
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let deleted_by_id = event
            .payload
            .get("deleted_by")
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        ArgStack::new()
            .set("message_id".to_owned(), Variant::String(message_id))
            .set("deleted_by_id".to_owned(), Variant::String(deleted_by_id))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn delete_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message_deleted",
            serde_json::json!({
                "message": { "id": "msg-uuid-999" },
                "deleted_by": { "id": 5 }
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_ids() {
        let stack = MessageDeletedDescriptor.build_arg_stack(&delete_event());
        assert_eq!(
            stack.get("message_id"),
            Some(&Variant::String("msg-uuid-999".to_owned()))
        );
        assert_eq!(
            stack.get("deleted_by_id"),
            Some(&Variant::String("5".to_owned()))
        );
    }
}
