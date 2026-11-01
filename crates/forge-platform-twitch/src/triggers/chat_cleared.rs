use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::chat_mod as chat_mod_fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text(
        "broadcaster_login",
        chat_mod_fields::BROADCASTER,
        chat_mod_fields::BROADCASTER_LOGIN,
    )
    .synthesis(SynthesisHint::Username),
    Variable::nested_text(
        "broadcaster_id",
        chat_mod_fields::BROADCASTER,
        chat_mod_fields::BROADCASTER_ID,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHAT_CLEARED,
    category: TriggerCategory::Chat,
    label: "Chat Cleared",
    summary: "Fires when all messages are cleared from the chat room",
    search_text: "twitch chat clear purge moderation all messages",
    icon: "eraser",
    kind: event_kinds::CHANNEL_CHAT_CLEAR,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn chat_clear_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.chat.clear",
            serde_json::json!({
                "broadcaster": { "id": "100", "login": "host_channel" }
            }),
        )
    }

    #[test]
    fn build_arg_stack_maps_broadcaster_from_nested_payload() {
        let stack = SPEC.build_arg_stack(&chat_clear_event());
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String("host_channel".to_owned()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String("100".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_defaults_to_empty_strings_when_broadcaster_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.chat.clear",
            serde_json::json!({}),
        );
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("broadcaster_login"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("broadcaster_id"),
            Some(&Variant::String(String::new()))
        );
    }
}
