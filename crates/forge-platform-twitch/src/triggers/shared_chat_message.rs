use super::spec::TwitchTriggerSpec;
use crate::vocabulary::trigger_ids;
use crate::vocabulary::{event_kinds, messages};
use forge_events::Event;
use forge_registry::{ActorDeclaration, TriggerCategory, TriggerVariables};
use forge_types::{DeclaredVariable, SynthesisHint, TriggerConfig, UserText, Variant, VariantKind};

use super::chat_arg_stack::base_chat_variables;
use crate::vocabulary::fields::chat as chat_fields;

fn condition_display(_: &TriggerConfig) -> UserText {
    messages::CONDITION_ANY_SHARED_CHAT_MESSAGE.into()
}

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    !from_channel_field(event, chat_fields::FROM_CHANNEL_LOGIN).is_empty()
}

fn variables() -> Option<TriggerVariables> {
    Some(
        base_chat_variables()
            .event_specific(
                DeclaredVariable {
                    name: "chat.from_channel.login".to_owned(),
                    kind: VariantKind::String,
                    synthesis: Some(SynthesisHint::Username),
                },
                |event| Variant::String(from_channel_field(event, chat_fields::FROM_CHANNEL_LOGIN)),
            )
            .event_specific(
                DeclaredVariable {
                    name: "chat.from_channel.display_name".to_owned(),
                    kind: VariantKind::String,
                    synthesis: Some(SynthesisHint::DisplayName),
                },
                |event| {
                    Variant::String(from_channel_field(
                        event,
                        chat_fields::FROM_CHANNEL_DISPLAY_NAME,
                    ))
                },
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SHARED_CHAT_MESSAGE_RECEIVED,
    category: TriggerCategory::Chat,
    label: "Shared chat message",
    summary: "Fires when a message arrives via a Shared Chat session from another channel",
    search_text: "twitch shared chat session source channel cross-channel",
    icon: "messages",
    kind: event_kinds::CHANNEL_CHAT_MESSAGE,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    condition_display,
    ..TwitchTriggerSpec::BASE
};

fn from_channel_field(event: &Event, key: &str) -> String {
    super::payload_read::nested_text(event, chat_fields::FROM_CHANNEL, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn chat_event(from_channel: Option<serde_json::Value>) -> Event {
        let mut payload = serde_json::json!({
            "channel": "host",
            "user": { "login": "guest_viewer", "id": "42", "roles": [] },
            "message": "hi from over there",
            "badges": [],
            "color": ""
        });
        if let Some(fc) = from_channel {
            payload["from_channel"] = fc;
        }
        Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload)
    }

    #[test]
    fn matches_trigger_requires_non_empty_from_channel_login() {
        let cases = [
            (
                "from_channel with login",
                Some(serde_json::json!({ "login": "other", "display_name": "Other" })),
                true,
            ),
            ("no from_channel key", None, false),
            (
                "empty login",
                Some(serde_json::json!({ "login": "", "display_name": "Other" })),
                false,
            ),
            (
                "login key missing",
                Some(serde_json::json!({ "display_name": "Other" })),
                false,
            ),
        ];
        for (name, fc, expected) in cases {
            assert_eq!(
                SPEC.matches_trigger(&TriggerConfig::new(), &chat_event(fc)),
                expected,
                "case: {name}"
            );
        }
    }

    #[test]
    fn build_arg_stack_adds_from_channel_args_to_base_chat_args() {
        let event = chat_event(Some(
            serde_json::json!({ "login": "other_chan", "display_name": "OtherChan" }),
        ));
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("chat.from_channel.login"),
            Some(&Variant::String("other_chan".to_owned()))
        );
        assert_eq!(
            stack.get("chat.from_channel.display_name"),
            Some(&Variant::String("OtherChan".to_owned()))
        );
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("hi from over there".to_owned()))
        );
    }
}
