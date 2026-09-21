use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::shared_chat as shared_chat_fields;

pub(crate) struct SharedChatSessionEndedDescriptor;

impl TriggerKindDescriptor for SharedChatSessionEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.shared_chat.session_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared Chat session ended"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster's channel leaves a shared chat session or the session ends"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session ended left host"
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
            kind_prefix: Some("twitch.channel.shared_chat.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), session_host_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "shared_chat.session_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Shared chat session ID".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shared_chat_fields::SHARED_CHAT,
                            shared_chat_fields::SESSION_ID,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "host_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Host channel login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shared_chat_fields::HOST,
                            shared_chat_fields::HOST_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "host_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Host channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shared_chat_fields::HOST,
                            shared_chat_fields::HOST_ID,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

fn session_host_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(shared_chat_fields::HOST),
        shared_chat_fields::HOST_ID,
        shared_chat_fields::HOST_LOGIN,
        shared_chat_fields::HOST_DISPLAY_NAME,
    )
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use forge_events::Event;
    use forge_types::ArgStack;

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
            "channel.shared_chat.end",
            serde_json::json!({
                "shared_chat": { "session_id": "sess-end" },
                "host": { "id": "300", "login": "host_c", "display_name": "HostC" },
            }),
        );
        let stack = SharedChatSessionEndedDescriptor.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "sess-end");
        assert_eq!(str_var(&stack, "host_login"), "host_c");
        assert_eq!(str_var(&stack, "host_id"), "300");
    }
}
