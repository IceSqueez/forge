use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::automod as automod_fields;

pub(crate) struct AutomodMessageHeldDescriptor;

impl TriggerKindDescriptor for AutomodMessageHeldDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.message_held"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod message held"
    }

    fn summary(&self) -> &str {
        "Fires when AutoMod holds a chat message for moderator review"
    }

    fn search_text(&self) -> &str {
        "twitch automod message held review moderation"
    }

    fn icon_name(&self) -> &str {
        "shield"
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
            kind_prefix: Some("twitch.automod.message.hold".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), held_author_identity)
                .message_text(|event| payload_read::text(event, automod_fields::MESSAGE_TEXT))
                .event_specific(
                    DeclaredVariable {
                        name: "automod.message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod message ID".to_owned(),
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
                        label: "Automod category".to_owned(),
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
                        label: "Automod level".to_owned(),
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
                        label: "Hold reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, automod_fields::REASON)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "automod.terms_found".to_owned(),
                        kind: VariantKind::Array,
                        label: "Blocked term IDs".to_owned(),
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
                .event_specific(
                    DeclaredVariable {
                        name: "held_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Held at".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            automod_fields::AUTOMOD,
                            automod_fields::HELD_AT,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

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
    fn event_filter_targets_automod_hold_kind_from_twitch() {
        let filter = AutomodMessageHeldDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.automod.message.hold")
        );
    }

    #[test]
    fn build_arg_stack_exposes_message_id_chaining_var_and_typed_level() {
        let stack = AutomodMessageHeldDescriptor.build_arg_stack(&hold_event());
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
            let stack = AutomodMessageHeldDescriptor.build_arg_stack(&event);
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
