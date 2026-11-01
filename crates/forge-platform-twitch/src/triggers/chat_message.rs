use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{DeliveryLane, Event};
use forge_registry::{
    ActorDeclaration, ChatTriggerFamily, FIRST_CHATTERS_ONLY, FormField, TriggerCategory,
    TriggerVariables, admits_chatter, chat_message_condition, first_chatters_only_field,
};
use forge_types::{TriggerConfig, UserText, Variant};

use super::chat_arg_stack::base_chat_variables;

fn default_config() -> TriggerConfig {
    TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), Variant::Bool(false))])
}

fn config_fields() -> Vec<FormField> {
    vec![first_chatters_only_field()]
}

fn condition_display(config: &TriggerConfig) -> UserText {
    chat_message_condition(config)
}

fn matches(config: &TriggerConfig, event: &Event) -> bool {
    admits_chatter(config, event)
}

fn variables() -> Option<TriggerVariables> {
    Some(base_chat_variables())
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHAT_MESSAGE,
    category: TriggerCategory::Chat,
    label: "Chat message",
    summary: "Fires for every message posted in chat",
    search_text: "twitch chat message trigger any incoming",
    icon: "message-circle",
    kind: event_kinds::CHANNEL_CHAT_MESSAGE,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    default_config,
    config_fields,
    condition_display,
    chat_family: Some(ChatTriggerFamily::Message),
    lane: DeliveryLane::Bulk,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
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
        let stack = SPEC.build_arg_stack(&chat_event("hi there"));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hi there".to_owned()))
        );
    }

    #[test]
    fn the_first_time_chatter_filter_admits_only_a_viewers_first_message() {
        let filtered = TriggerConfig::from([(FIRST_CHATTERS_ONLY.to_owned(), Variant::Bool(true))]);
        for (config, first_message, admitted) in [
            (SPEC.default_config(), false, true),
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
                SPEC.matches_trigger(&config, &event),
                admitted,
                "config {config:?} first {first_message}"
            );
        }
    }
}
