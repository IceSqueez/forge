use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, SynthesisHint, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::automod as automod_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), held_author_identity)
            .message_text(|event| payload_read::text(event, automod_fields::MESSAGE_TEXT))
            .event_specific(
                DeclaredVariable {
                    name: "automod.message_id".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        automod_fields::AUTOMOD,
                        automod_fields::MESSAGE_ID,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "automod.category".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| {
                    Variant::String(payload_read::nested_text(
                        event,
                        automod_fields::AUTOMOD,
                        automod_fields::CATEGORY,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "automod.level".to_owned(),
                    kind: VariantKind::Int,
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 4 }),
                },
                |event| {
                    Variant::Int(payload_read::nested_number(
                        event,
                        automod_fields::AUTOMOD,
                        automod_fields::LEVEL,
                    ))
                },
            )
            .event_specific(
                DeclaredVariable {
                    name: "automod.reason".to_owned(),
                    kind: VariantKind::String,
                    synthesis: None,
                },
                |event| Variant::String(payload_read::text(event, automod_fields::REASON)),
            )
            .event_specific(
                DeclaredVariable {
                    name: "automod.terms_found".to_owned(),
                    kind: VariantKind::Array,
                    synthesis: None,
                },
                |event| {
                    payload_read::nested_text_list(
                        event,
                        automod_fields::BLOCKED_TERM,
                        automod_fields::TERMS_FOUND,
                    )
                },
            )
            .event_specific_optional(
                DeclaredVariable {
                    name: "held_at".to_owned(),
                    kind: VariantKind::Datetime,
                    synthesis: None,
                },
                |event| {
                    payload_read::nested_timestamp(
                        event,
                        automod_fields::AUTOMOD,
                        automod_fields::HELD_AT,
                    )
                },
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::AUTOMOD_MESSAGE_HELD,
    category: TriggerCategory::Moderation,
    label: "AutoMod message held",
    summary: "Fires when AutoMod holds a chat message for moderator review",
    search_text: "twitch automod message held review moderation",
    icon: "shield",
    kind: event_kinds::AUTOMOD_MESSAGE_HOLD,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn held_author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(automod_fields::USER),
        automod_fields::USER_ID,
        automod_fields::USER_LOGIN,
        automod_fields::USER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

    fn hold_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.message.hold",
            serde_json::json!({
                "automod": {
                    "message_id": "hold-abc-123",
                    "category": "harassment",
                    "level": 3,
                    "held_at": "2026-06-13T20:00:00Z",
                },
                "user": {
                    "id": "777",
                    "login": "viewer_one",
                    "display_name": "ViewerOne",
                },
                "message_text": "borderline message",
            }),
        )
    }

    #[test]
    fn build_arg_stack_exposes_message_id_chaining_var_and_typed_level() {
        let stack = SPEC.build_arg_stack(&hold_event());
        assert_eq!(
            stack.get("automod.message_id"),
            Some(&Variant::String("hold-abc-123".to_owned()))
        );
        assert_eq!(stack.get("automod.level"), Some(&Variant::Int(3)));
        assert_eq!(
            stack.get("message_text"),
            Some(&Variant::String("borderline message".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_reads_blocked_term_fields_and_defaults_them_on_classification_holds() {
        let blocked = Event::new(
            EventSource::Twitch,
            "twitch.automod.message.hold",
            serde_json::json!({
                "automod": { "message_id": "hold-blocked", "category": null, "level": null },
                "reason": "blocked_term",
                "blocked_term": { "terms_found": ["term-1", 42, "term-2"] },
            }),
        );

        for (event, reason, terms) in [
            (blocked, "blocked_term", vec!["term-1", "term-2"]),
            (hold_event(), "", vec![]),
        ] {
            let stack = SPEC.build_arg_stack(&event);
            assert_eq!(
                stack.get("automod.reason"),
                Some(&Variant::String(reason.to_owned())),
            );
            assert_eq!(
                stack.get("automod.terms_found"),
                Some(&Variant::Array(
                    terms
                        .into_iter()
                        .map(|t| Variant::String(t.to_owned()))
                        .collect()
                )),
                "terms for reason {reason:?}",
            );
        }
    }
}
