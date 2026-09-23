use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::chat_arg_stack::base_chat_variables;
use crate::payload_fields::chat as chat_fields;

pub(crate) struct SharedChatMessageDescriptor;

impl TriggerKindDescriptor for SharedChatMessageDescriptor {
    fn id(&self) -> &str {
        "twitch.shared_chat.message_received"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared chat message"
    }

    fn summary(&self) -> &str {
        "Fires when a message arrives via a Shared Chat session from another channel"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session source channel cross-channel"
    }

    fn icon_name(&self) -> &str {
        "messages"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any shared chat message".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.channel.chat.message".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
            .payload
            .get(chat_fields::FROM_CHANNEL)
            .and_then(|fc| fc.get(chat_fields::FROM_CHANNEL_LOGIN))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            base_chat_variables()
                .event_specific(
                    DeclaredVariable {
                        name: "chat.from_channel.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Source channel login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    |event| {
                        Variant::String(from_channel_field(event, chat_fields::FROM_CHANNEL_LOGIN))
                    },
                )
                .event_specific(
                    DeclaredVariable {
                        name: "chat.from_channel.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Source channel display name".to_owned(),
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

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn from_channel_field(event: &Event, key: &str) -> String {
    event
        .payload
        .get(chat_fields::FROM_CHANNEL)
        .and_then(|from_channel| from_channel.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
                SharedChatMessageDescriptor.matches_trigger(&TriggerConfig::new(), &chat_event(fc)),
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
        let stack = SharedChatMessageDescriptor.build_arg_stack(&event);
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
