use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::payload_fields::moderation as fields;

pub(crate) struct BanDescriptor;

impl TriggerKindDescriptor for BanDescriptor {
    fn id(&self) -> &str {
        "kick.moderation.banned"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "User banned"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator bans a user in Kick chat"
    }

    fn search_text(&self) -> &str {
        "kick ban timeout moderator user removed"
    }

    fn icon_name(&self) -> &str {
        "user-x"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.moderation.banned".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(kick_actor(ActorRole::Principal), banned_identity)
                .actor(kick_actor(ActorRole::Moderator), moderator_identity)
                .event_specific(
                    DeclaredVariable {
                        name: "duration_secs".to_owned(),
                        kind: VariantKind::Int,
                        label: "Ban duration (seconds)".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt {
                            min: 0,
                            max: 1_209_600,
                        }),
                    },
                    |event| Variant::Int(payload_read::number(event, fields::DURATION_SECS)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "reason".to_owned(),
                        kind: VariantKind::String,
                        label: "Ban reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REASON)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "banned_user_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned user ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| Variant::String(banned_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "banned_username".to_owned(),
                        kind: VariantKind::String,
                        label: "Banned username".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| Variant::String(payload_read::login_of(banned_identity(event))),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn banned_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::BANNED_USER))
}

fn moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::MODERATOR))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ban_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.moderation.banned",
            serde_json::json!({
                "banned_user": { "id": 77, "username": "bad_actor" },
                "moderator": { "id": 2, "username": "mod" },
                "is_permanent": false,
                "duration_secs": 300,
                "reason": null
            }),
        )
    }

    #[test]
    fn build_arg_stack_extracts_ban_fields() {
        let stack = BanDescriptor.build_arg_stack(&ban_event());
        assert_eq!(
            stack.get("banned_user_id"),
            Some(&Variant::String("77".to_owned()))
        );
        assert_eq!(
            stack.get("banned_username"),
            Some(&Variant::String("bad_actor".to_owned()))
        );
        assert_eq!(stack.get("duration_secs"), Some(&Variant::Int(300)));
    }
}
