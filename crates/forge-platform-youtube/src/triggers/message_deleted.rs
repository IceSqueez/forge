use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "chat.message_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Deleted message ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "chat.target_user.channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Deleted message author channel ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "chat.moderator.channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Moderator channel ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn deleted_event(payload: serde_json::Value) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message_deleted",
            payload,
        )
    }

    #[test]
    fn build_arg_stack_surfaces_message_id_and_moderator() {
        let event = deleted_event(json!({
            "chat.message_id": "msg-removed-1",
            "chat.target_user.channel_id": "",
            "chat.moderator.channel_id": "UCmod",
        }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String("msg-removed-1".to_owned()))
        );
        assert_eq!(
            stack.get("chat.moderator.channel_id"),
            Some(&Variant::String("UCmod".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_passes_through_unsourced_target_user_as_empty() {
        let event = deleted_event(json!({
            "chat.message_id": "msg-removed-2",
            "chat.target_user.channel_id": "",
            "chat.moderator.channel_id": "UCmod",
        }));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.target_user.channel_id"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_defaults_every_key_to_empty() {
        let event = deleted_event(json!({}));

        let stack = ChatMessageDeletedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("chat.message_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("chat.target_user.channel_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("chat.moderator.channel_id"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn event_filter_targets_message_deleted_kind_on_youtube() {
        let filter = ChatMessageDeletedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::YouTube));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("youtube.chat.message_deleted")
        );
    }
}
