use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalVariable, EventFilter,
    FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, kick_actor};
use crate::vocabulary::fields::moderation as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct BanDescriptor;

impl TriggerKindDescriptor for BanDescriptor {
    fn id(&self) -> &str {
        trigger_ids::MODERATION_BANNED
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
            kind_prefix: Some(event_kinds::MODERATION_BANNED.to_owned()),
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
                        name: variables::ban::DURATION_SECS.to_owned(),
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
                        name: variables::ban::REASON.to_owned(),
                        kind: VariantKind::String,
                        label: "Ban reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, fields::REASON)),
                )
                .legacy(
                    variables::legacy::BANNED_USER_ID,
                    "Banned user ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                )
                .legacy(
                    variables::legacy::BANNED_USERNAME,
                    "Banned username",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
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
