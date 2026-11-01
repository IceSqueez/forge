use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, TriggerCategory,
    TriggerVariables,
};
use forge_types::{DeclaredVariable, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::shared_chat as shared_chat_fields;

fn variables() -> Option<TriggerVariables> {
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
                "host_login",
                "Host channel login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "host_id",
                "Host channel ID",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SHARED_CHAT_SESSION_ENDED,
    category: TriggerCategory::Chat,
    label: "Shared Chat session ended",
    summary: "Fires when the broadcaster's channel leaves a shared chat session or the session ends",
    search_text: "twitch shared chat session ended left host",
    icon: "chat",
    kind: event_kinds::CHANNEL_SHARED_CHAT_END,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn session_host_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(shared_chat_fields::HOST),
        shared_chat_fields::HOST_ID,
        shared_chat_fields::HOST_LOGIN,
        shared_chat_fields::HOST_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
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
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "sess-end");
        assert_eq!(str_var(&stack, "host_login"), "host_c");
        assert_eq!(str_var(&stack, "host_id"), "300");
    }
}
