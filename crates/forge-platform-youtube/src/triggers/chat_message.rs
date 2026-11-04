use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorRole, ActorSlot, CanonicalVariable, ChatTriggerFamily, EventFilter,
    FIRST_CHATTERS_ONLY, FormField, KindPlatformContract, TriggerCatalogEntry, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables, admits_chatter, chat_message_condition,
    first_chatters_only_field,
};
use forge_types::{PlatformId, TriggerConfig, UserText, Variant};

use super::payload_read::{self, author_avatar_url, youtube_actor};
use crate::vocabulary::fields::chat as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ChatMessageDescriptor;

impl TriggerCatalogEntry for ChatMessageDescriptor {
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
}

impl TriggerKindDescriptor for ChatMessageDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHAT_MESSAGE
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

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        chat_message_condition(config)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::CHAT_MESSAGE.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        admits_chatter(config, event)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(
                    youtube_actor(ActorRole::Principal),
                    payload_read::author_identity,
                )
                .message_text(|event| payload_read::text(event, fields::MESSAGE_TEXT))
                .first_chat_message()
                .chat_message_id(|event| payload_read::text(event, fields::MESSAGE_ID))
                .chat_author_look(author_avatar_url)
                .legacy(
                    variables::sender::DISPLAY_NAME,
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                )
                .legacy(
                    variables::sender::CHANNEL_ID,
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::ChatViewer;

    fn chat_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
            serde_json::json!({
                "message_id": "LCC.msg-1",
                "message_text": "hello world",
                "author": { "display_name": "Viewer One", "channel_id": "UCxyz" }
            }),
        )
    }

    #[test]
    fn a_youtube_chat_message_publishes_no_emote_codes() {
        let event = Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
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

        let stack = ChatMessageDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("message_emotes"), None);
    }

    #[test]
    fn a_chat_message_publishes_its_author_as_the_canonical_principal_and_its_message_id() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event());
        for (name, value) in [
            ("user_id", "UCxyz"),
            ("user_name", "Viewer One"),
            ("message_text", "hello world"),
            ("message_id", "LCC.msg-1"),
            ("chat.message_id", "LCC.msg-1"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
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

    fn published_chat_line() -> Event {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/published_chat_message.json"
        ))
        .unwrap();
        Event::new(
            EventSource::YouTube,
            "youtube.chat.message",
            fixture["payload"].clone(),
        )
    }

    #[test]
    fn a_polled_line_publishes_the_members_badge_the_profile_image_and_an_empty_name_color() {
        let stack = ChatMessageDescriptor.build_arg_stack(&published_chat_line());

        assert_eq!(
            (
                stack.get("user_badges"),
                stack.get("user_avatar"),
                stack.get("user_color"),
            ),
            (
                Some(&Variant::String("MEMBER".to_owned())),
                Some(&Variant::String("https://yt3.ggpht.com/avatar".to_owned())),
                Some(&Variant::String(String::new())),
            )
        );
    }

    #[test]
    fn a_line_without_a_profile_image_publishes_an_empty_avatar() {
        for avatar in [None, Some(serde_json::Value::Null)] {
            let mut event = chat_event();
            if let Some(avatar) = avatar.clone() {
                event.payload["author_avatar_url"] = avatar;
            }

            let stack = ChatMessageDescriptor.build_arg_stack(&event);

            assert_eq!(
                stack.get("user_avatar"),
                Some(&Variant::String(String::new())),
                "avatar field {avatar:?}"
            );
        }
    }
}
