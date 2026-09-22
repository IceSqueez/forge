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

    use serde_json::json;

    fn ban_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Kick, "kick.moderation.banned", payload)
    }

    fn a_five_minute_timeout() -> Event {
        ban_event(json!({
            "banned_user": { "id": 77, "username": "bad_actor" },
            "moderator": { "id": 2, "username": "mod" },
            "is_permanent": false,
            "duration_secs": 300,
            "reason": null
        }))
    }

    #[test]
    fn a_ban_names_the_banned_user_as_principal_and_the_moderator_in_its_own_block() {
        let stack = BanDescriptor.build_arg_stack(&a_five_minute_timeout());
        for (name, value) in [
            ("user_id", "77"),
            ("user_login", "bad_actor"),
            ("moderator_id", "2"),
            ("moderator_login", "mod"),
            ("moderator_platform", "kick"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn a_ban_actor_the_wire_gives_no_display_name_is_shown_under_the_login() {
        let stack = BanDescriptor.build_arg_stack(&a_five_minute_timeout());
        assert_eq!(
            stack.get("user_name"),
            Some(&Variant::String("bad_actor".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_name"),
            Some(&Variant::String("mod".to_owned()))
        );
    }

    #[test]
    fn the_legacy_ban_names_still_carry_what_their_canonical_twins_carry() {
        let stack = BanDescriptor.build_arg_stack(&a_five_minute_timeout());
        assert_eq!(stack.get("banned_user_id"), stack.get("user_id"));
        assert_eq!(stack.get("banned_username"), stack.get("user_login"));
        assert_eq!(
            stack.get("banned_user_id"),
            Some(&Variant::String("77".to_owned()))
        );
    }

    #[test]
    fn the_ban_terms_come_from_the_payload_with_a_permanent_ban_reading_as_zero_seconds() {
        for (duration, reason, expected_secs, expected_reason) in [
            (json!(300), json!("spam"), 300, "spam"),
            (json!(0), json!(null), 0, ""),
            (json!(1_209_600), json!("raid bot"), 1_209_600, "raid bot"),
        ] {
            let stack = BanDescriptor.build_arg_stack(&ban_event(json!({
                "banned_user": { "id": 77, "username": "bad_actor" },
                "duration_secs": duration.clone(),
                "reason": reason.clone()
            })));
            assert_eq!(
                stack.get("duration_secs"),
                Some(&Variant::Int(expected_secs)),
                "duration {duration}"
            );
            assert_eq!(
                stack.get("reason"),
                Some(&Variant::String(expected_reason.to_owned())),
                "reason {reason}"
            );
        }
    }
}
