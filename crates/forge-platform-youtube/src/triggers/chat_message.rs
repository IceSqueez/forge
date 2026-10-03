use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ChatTriggerFamily, EventFilter, FIRST_CHATTERS_ONLY,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
    admits_chatter, chat_message_condition, first_chatters_only_field,
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some("youtube.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        admits_chatter(config, event)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), author_identity)
                .message_text(|event| payload_read::text(event, fields::MESSAGE_TEXT))
                .first_chat_message()
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

    fn delivery_lane(&self) -> DeliveryLane {
        DeliveryLane::Bulk
    }
}

fn author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::AUTHOR))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::ChatViewer;

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
    fn a_chat_message_publishes_its_author_as_the_canonical_principal() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event());
        for (name, value) in [
            ("user_id", "UCxyz"),
            ("user_name", "Viewer One"),
            ("message_text", "hello world"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn the_legacy_author_names_still_carry_what_their_canonical_twins_carry() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event());
        assert_eq!(stack.get("user_display_name"), stack.get("user_name"));
        assert_eq!(stack.get("channel_id"), stack.get("user_id"));
        assert_eq!(
            stack.get("user_display_name"),
            Some(&Variant::String("Viewer One".to_owned()))
        );
    }

    #[test]
    fn the_first_time_chatter_filter_admits_only_a_viewers_first_message() {
        let filtered = TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), Variant::Bool(true))]);
        for (config, first_message, admitted) in [
            (ChatMessageDescriptor.default_config(), false, true),
            (filtered.clone(), true, true),
            (filtered, false, false),
        ] {
            let mut event = chat_event();
            ChatViewer {
                first_message,
                ..ChatViewer::new("42", "viewer")
            }
            .attach(&mut event.payload);
            assert_eq!(
                ChatMessageDescriptor.matches_trigger(&config, &event),
                admitted,
                "config {config:?} first {first_message}"
            );
        }
    }
}
