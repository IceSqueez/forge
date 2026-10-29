use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FIRST_CHATTERS_ONLY,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    admits_chatter, chat_message_condition, first_chatters_only_field,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, TriggerConfig, Variant,
    VariantKind,
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
        TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), Variant::Bool(false))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![first_chatters_only_field()]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        chat_message_condition(config)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.chat.message.sent".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        admits_chatter(config, event)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), sender_identity)
                .message_text(|event| payload_read::text(event, fields::CONTENT))
                .first_chat_message()
                .chat_message_id(|event| payload_read::text(event, fields::MESSAGE_ID))
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
                    "sender_id",
                    "Sender user ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    "username",
                    "Sender username",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                )
                .legacy(
                    "display_name",
                    "Sender display name",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy("content", "Message content", CanonicalVariable::MessageText),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Message)
    }

    fn delivery_lane(&self) -> DeliveryLane {
        DeliveryLane::Bulk
    }
}

pub(super) fn sender_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::SENDER))
}

pub(super) fn sender_color(event: &Event) -> String {
    payload_read::nested_text(event.payload.get(fields::SENDER), fields::COLOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::ChatViewer;

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
    fn a_kick_chat_message_publishes_no_emote_codes() {
        let event = Event::new(
            EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({
                "message_text": "LUL",
                "content": "LUL",
                forge_types::ChatPayload::KEY: {
                    "platform_msg_id": "m-1",
                    "author": "viewer",
                    "author_color": null,
                    "segments": [{ "type": "emote", "id": "1", "name": "LUL" }],
                    "badges": [],
                    "is_event": false,
                    "event_detail": null,
                },
            }),
        );

        let stack = ChatDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("message_emotes"), None);
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
    fn a_sender_the_wire_gives_no_display_name_is_shown_under_the_login_in_every_name_slot() {
        let stack = ChatDescriptor.build_arg_stack(&chat_event(json!({
            "content": "hi",
            "sender": { "id": 42, "username": "viewer_slug", "display_name": "" }
        })));
        for name in ["user_name", "user_login", "display_name"] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String("viewer_slug".to_owned())),
                "'{name}'"
            );
        }
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
            for name in ["message_id", "chat.message_id"] {
                assert_eq!(
                    stack.get(name),
                    Some(&Variant::String("msg-1".to_owned())),
                    "'{name}'"
                );
            }
            assert_eq!(
                stack.get("color"),
                Some(&Variant::String("#00FF00".to_owned()))
            );
        }
    }

    #[test]
    fn the_first_time_chatter_filter_admits_only_a_viewers_first_message() {
        let filtered = TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), Variant::Bool(true))]);
        for (config, first_message, admitted) in [
            (ChatDescriptor.default_config(), false, true),
            (filtered.clone(), true, true),
            (filtered, false, false),
        ] {
            let mut event = a_message_from_a_named_viewer();
            ChatViewer {
                first_message,
                ..ChatViewer::new("42", "viewer")
            }
            .attach(&mut event.payload);
            assert_eq!(
                ChatDescriptor.matches_trigger(&config, &event),
                admitted,
                "config {config:?} first {first_message}"
            );
        }
    }
}
