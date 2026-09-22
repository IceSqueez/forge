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

    use serde_json::json;

    fn chat_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Kick, "kick.chat.message.sent", payload)
    }

    fn a_message_from_a_named_viewer() -> Event {
        chat_event(json!({
            "message_id": "msg-1",
            "content": "hello stream",
            "reply_to_message_id": null,
            "sender": {
                "id": 42,
                "username": "viewer_slug",
                "display_name": "Viewer Display",
                "color": "#00FF00"
            }
        }))
    }

    #[test]
    fn the_login_and_the_display_name_never_swap_slots() {
        let stack = ChatDescriptor.build_arg_stack(&a_message_from_a_named_viewer());
        for (name, value) in [
            ("user_id", "42"),
            ("user_login", "viewer_slug"),
            ("user_name", "Viewer Display"),
            ("user_platform", "kick"),
            ("message_text", "hello stream"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn a_sender_the_wire_gives_no_display_name_is_shown_under_the_login() {
        let stack = ChatDescriptor.build_arg_stack(&chat_event(json!({
            "content": "hi",
            "sender": { "id": 42, "username": "viewer_slug" }
        })));
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
    }

    #[test]
    fn the_legacy_chat_names_still_carry_what_their_canonical_twins_carry() {
        let stack = ChatDescriptor.build_arg_stack(&a_message_from_a_named_viewer());
        assert_eq!(stack.get("sender_id"), stack.get("user_id"));
        assert_eq!(stack.get("username"), stack.get("user_login"));
        assert_eq!(stack.get("display_name"), stack.get("user_name"));
        assert_eq!(stack.get("content"), stack.get("message_text"));
        assert_eq!(
            stack.get("username"),
            Some(&Variant::String("viewer_slug".to_owned()))
        );
        assert_eq!(
            stack.get("display_name"),
            Some(&Variant::String("Viewer Display".to_owned()))
        );
    }

    #[test]
    fn the_message_envelope_fields_are_read_straight_from_the_payload() {
        for (reply_wire, expected_reply) in [
            (json!("parent-99"), "parent-99"),
            (json!(null), ""),
            (json!(7), ""),
        ] {
            let stack = ChatDescriptor.build_arg_stack(&chat_event(json!({
                "message_id": "msg-1",
                "content": "hello stream",
                "reply_to_message_id": reply_wire.clone(),
                "sender": { "id": 42, "color": "#00FF00" }
            })));
            assert_eq!(
                stack.get("reply_to_id"),
                Some(&Variant::String(expected_reply.to_owned())),
                "reply wire {reply_wire}"
            );
            assert_eq!(
                stack.get("message_id"),
                Some(&Variant::String("msg-1".to_owned()))
            );
            assert_eq!(
                stack.get("color"),
                Some(&Variant::String("#00FF00".to_owned()))
            );
        }
    }
}
