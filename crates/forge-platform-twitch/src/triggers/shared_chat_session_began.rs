use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::shared_chat as shared_chat_fields;

pub(crate) struct SharedChatSessionBeganDescriptor;

impl TriggerKindDescriptor for SharedChatSessionBeganDescriptor {
    fn id(&self) -> &str {
        "twitch.shared_chat.session_began"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared Chat session began"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster's channel joins an active shared chat session"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session began started host"
    }

    fn icon_name(&self) -> &str {
        "chat"
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
        "any".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("channel.shared_chat.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let shared_chat = event.payload.get(shared_chat_fields::SHARED_CHAT);
        let host = event.payload.get(shared_chat_fields::HOST);

        let session_id = shared_chat
            .and_then(|s| s.get(shared_chat_fields::SESSION_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let host_login = host
            .and_then(|h| h.get(shared_chat_fields::HOST_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let host_id = host
            .and_then(|h| h.get(shared_chat_fields::HOST_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "shared_chat.session_id".to_owned(),
                Variant::String(session_id),
            )
            .set("host_login".to_owned(), Variant::String(host_login))
            .set("host_id".to_owned(), Variant::String(host_id))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "shared_chat.session_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Shared chat session ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "host_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Host channel login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "host_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Host channel ID".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use forge_events::Event;

    fn str_var(stack: &ArgStack, key: &str) -> String {
        match stack.get(key) {
            Some(Variant::String(s)) => s.clone(),
            other => panic!("expected String at {key}, got {other:?}"),
        }
    }

    #[test]
    fn build_arg_stack_extracts_session_id_and_host_from_nested_payload() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.shared_chat.begin",
            serde_json::json!({
                "shared_chat": { "session_id": "sess-abc" },
                "host": { "id": "100", "login": "host_chan", "display_name": "Host" },
            }),
        );
        let stack = SharedChatSessionBeganDescriptor.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "sess-abc");
        assert_eq!(str_var(&stack, "host_login"), "host_chan");
        assert_eq!(str_var(&stack, "host_id"), "100");
    }

    #[test]
    fn build_arg_stack_yields_empty_strings_when_payload_objects_absent() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.shared_chat.begin",
            serde_json::json!({}),
        );
        let stack = SharedChatSessionBeganDescriptor.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "");
        assert_eq!(str_var(&stack, "host_login"), "");
        assert_eq!(str_var(&stack, "host_id"), "");
    }
}
