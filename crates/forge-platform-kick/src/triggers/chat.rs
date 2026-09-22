use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::chat as fields;

pub(crate) struct ChatDescriptor;

impl TriggerKindDescriptor for ChatDescriptor {
    fn id(&self) -> &str {
        "kick.chat.message.sent"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in Kick live chat"
    }

    fn search_text(&self) -> &str {
        "kick chat message trigger any incoming live"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
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
            kind_prefix: Some("kick.chat.message.sent".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), sender_identity)
                .message_text(|event| payload_read::text(event, fields::CONTENT))
                .event_specific(
                    DeclaredVariable {
                        name: "message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::MESSAGE_ID)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "color".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender name color".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(sender_color(event)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reply_to_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Replied-to message ID".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REPLY_TO_MESSAGE_ID)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "sender_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender user ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(sender_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "username".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(sender_identity(event))),
                )
                .legacy(
                    DeclaredVariable {
                        name: "display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Sender display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| Variant::String(sender_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "content".to_owned(),
                        kind: VariantKind::String,
                        label: "Message content".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    CanonicalVariable::MessageText,
                    |event| Variant::String(payload_read::text(event, fields::CONTENT)),
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

pub(super) fn sender_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::SENDER))
}

pub(super) fn sender_color(event: &Event) -> String {
    payload_read::nested_text(event.payload.get(fields::SENDER), fields::COLOR)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn chat_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({
                "message_id": "msg-1",
                "content": "hello stream",
                "reply_to_message_id": null,
                "sender": {
                    "id": 42,
                    "username": "viewer_slug",
                    "display_name": "Viewer Display",
                    "color": "#00FF00"
                }
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(ChatDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event()));
    }

    #[test]
    fn build_arg_stack_extracts_fields() {
        let stack = ChatDescriptor.build_arg_stack(&chat_event());
        assert_eq!(
            stack.get("message_id"),
            Some(&Variant::String("msg-1".to_owned()))
        );
        assert_eq!(
            stack.get("sender_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
        assert_eq!(
            stack.get("content"),
            Some(&Variant::String("hello stream".to_owned()))
        );
        assert_eq!(
            stack.get("color"),
            Some(&Variant::String("#00FF00".to_owned()))
        );
        assert_eq!(
            stack.get("reply_to_id"),
            Some(&Variant::String(String::new()))
        );
    }
}
