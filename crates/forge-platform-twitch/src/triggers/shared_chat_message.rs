use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use super::chat_arg_stack::{base_chat_args, base_chat_schema};
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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let from_login = event
            .payload
            .get(chat_fields::FROM_CHANNEL)
            .and_then(|fc| fc.get(chat_fields::FROM_CHANNEL_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_display_name = event
            .payload
            .get(chat_fields::FROM_CHANNEL)
            .and_then(|fc| fc.get(chat_fields::FROM_CHANNEL_DISPLAY_NAME))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        base_chat_args(event)
            .set(
                "chat.from_channel.login".to_owned(),
                Variant::String(from_login),
            )
            .set(
                "chat.from_channel.display_name".to_owned(),
                Variant::String(from_display_name),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        let mut schema = base_chat_schema();
        schema.variables.push(DeclaredVariable {
            name: "chat.from_channel.login".to_owned(),
            kind: VariantKind::String,
            label: "Source channel login".to_owned(),
            synthesis: Some(SynthesisHint::Username),
        });
        schema.variables.push(DeclaredVariable {
            name: "chat.from_channel.display_name".to_owned(),
            kind: VariantKind::String,
            label: "Source channel display name".to_owned(),
            synthesis: Some(SynthesisHint::DisplayName),
        });
        Some(schema)
    }
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
