use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::chat as fields;

pub(crate) struct ChatMessageDescriptor;

impl TriggerKindDescriptor for ChatMessageDescriptor {
    fn id(&self) -> &str {
        "youtube.chat.message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in YouTube live chat"
    }

    fn search_text(&self) -> &str {
        "youtube chat message trigger any incoming live"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
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
            kind_prefix: Some("youtube.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .message_text(|event| payload_read::text(event, fields::MESSAGE_TEXT))
                .legacy(
                    DeclaredVariable {
                        name: "user_display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(author_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(author_identity(event).id),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Message)
    }
}

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::AUTHOR))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
            serde_json::json!({
                "message_text": "hello world",
                "author": { "display_name": "Viewer One", "channel_id": "UCxyz" }
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ChatMessageDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event()));
    }

    #[test]
    fn build_arg_stack_extracts_fields() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event());
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hello world".to_owned()))
        );
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("Viewer One".to_owned()))
        );
        assert_eq!(
            stack.get("channel_id"),
            Some(&Variant::String("UCxyz".to_owned()))
        );
    }
}
