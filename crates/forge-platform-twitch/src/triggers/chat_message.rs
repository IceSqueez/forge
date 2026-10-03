use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{
    ActorDeclaration, ChatTriggerFamily, EventFilter, FIRST_CHATTERS_ONLY, FormField,
    KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables, admits_chatter,
    chat_message_condition, first_chatters_only_field,
};
use forge_types::{PlatformId, TriggerConfig, Variant};

use super::chat_arg_stack::base_chat_variables;

pub(crate) struct ChatMessageDescriptor;

impl TriggerKindDescriptor for ChatMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.chat.message"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires for every message posted in chat"
    }

    fn search_text(&self) -> &str {
        "twitch chat message trigger any incoming"
    }

    fn icon_name(&self) -> &str {
        "message-circle"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
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
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        admits_chatter(config, event)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(base_chat_variables())
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_types::ChatViewer;
    use forge_types::Variant;

    fn chat_event(msg: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({
                "channel": "mychannel",
                "user": { "login": "bob", "id": "456", "roles": [] },
                "message": msg,
                "badges": [],
                "color": "#FF0000"
            }),
        )
    }

    #[test]
    fn build_arg_stack_includes_base_chat_args() {
        let stack = ChatMessageDescriptor.build_arg_stack(&chat_event("hi there"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hi there".to_owned()))
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
            let mut event = chat_event("hello");
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
