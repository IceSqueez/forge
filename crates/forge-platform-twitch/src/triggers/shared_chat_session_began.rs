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
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "host_id",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::SHARED_CHAT_SESSION_BEGAN,
    category: TriggerCategory::Chat,
    label: "Shared Chat session began",
    summary: "Fires when the broadcaster's channel joins an active shared chat session",
    search_text: "twitch shared chat session began started host",
    icon: "chat",
    kind: event_kinds::CHANNEL_SHARED_CHAT_BEGIN,
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
            "channel.shared_chat.begin",
            serde_json::json!({
                "shared_chat": { "session_id": "sess-abc" },
                "host": { "id": "100", "login": "host_chan", "display_name": "Host" },
            }),
        );
        let stack = SPEC.build_arg_stack(&event);
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
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "");
        assert_eq!(str_var(&stack, "host_login"), "");
        assert_eq!(str_var(&stack, "host_id"), "");
    }
}
