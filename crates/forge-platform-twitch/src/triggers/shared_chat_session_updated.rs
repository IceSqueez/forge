use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::shared_chat as shared_chat_fields;

pub(crate) struct SharedChatSessionUpdatedDescriptor;

impl TriggerKindDescriptor for SharedChatSessionUpdatedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::SHARED_CHAT_SESSION_UPDATED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Shared Chat session updated"
    }

    fn summary(&self) -> &str {
        "Fires when the active shared chat session the broadcaster's channel is in changes"
    }

    fn search_text(&self) -> &str {
        "twitch shared chat session updated changed host"
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
            kind_prefix: Some(event_kinds::CHANNEL_SHARED_CHAT_UPDATE.to_owned()),
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
    fn build_arg_stack_extracts_session_id_and_host_login_from_nested_payload() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.shared_chat.update",
            serde_json::json!({
                "shared_chat": { "session_id": "sess-upd" },
                "host": { "id": "200", "login": "host_b", "display_name": "HostB" },
            }),
        );
        let stack = SharedChatSessionUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "shared_chat.session_id"), "sess-upd");
        assert_eq!(str_var(&stack, "host_login"), "host_b");
    }

    #[test]
    fn build_arg_stack_binds_host_id_from_host_payload() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.shared_chat.update",
            serde_json::json!({
                "shared_chat": { "session_id": "sess-upd" },
                "host": { "id": "200", "login": "host_b" },
            }),
        );
        let stack = SharedChatSessionUpdatedDescriptor.build_arg_stack(&event);
        assert_eq!(str_var(&stack, "host_id"), "200");
    }
}
