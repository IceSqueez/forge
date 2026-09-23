use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::automod as automod_fields;

pub(crate) struct AutomodMessageUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodMessageUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.message_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod message decision updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator approves, denies, or allows a held AutoMod message to expire"
    }

    fn search_text(&self) -> &str {
        "twitch automod message approved denied expired moderator held decision status"
    }

    fn icon_name(&self) -> &str {
        "shield"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "status_filter".to_owned(),
            Variant::String("any".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "status_filter",
            label: "Decision status",
            options: &["any", "approved", "denied", "expired"],
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let status = config
            .get("status_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("any");
        if status == "any" {
            "any status".to_owned()
        } else {
            format!("status = {}", status)
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some("twitch.automod.message.update".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        let filter = config
            .get("status_filter")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .unwrap_or("any");

        if filter == "any" {
            return true;
        }

        let event_status = event
            .payload
            .get(automod_fields::AUTOMOD)
            .and_then(|a| a.get(automod_fields::STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Twitch sends the decision status in Title Case ("Approved"); filter options are lowercase.
        event_status.to_lowercase() == filter
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), updated_author_identity)
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    automod_moderator_identity,
                )
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
                        name: "automod.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod status".to_owned(),
                        synthesis: None,
                    },
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            automod_fields::AUTOMOD,
                            automod_fields::STATUS,
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
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn updated_author_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(automod_fields::USER),
        automod_fields::USER_ID,
        automod_fields::USER_LOGIN,
        automod_fields::USER_DISPLAY_NAME,
    )
}

fn automod_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(automod_fields::MODERATOR),
        automod_fields::MODERATOR_ID,
        automod_fields::MODERATOR_LOGIN,
        automod_fields::MODERATOR_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_event(status: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.message.update",
            serde_json::json!({
                "automod": {
                    "message_id": "msg-77",
                    "status": status,
                    "category": "harassment",
                    "level": 4,
                },
                "user": {
                    "id": "777",
                    "login": "viewer_one",
                    "display_name": "ViewerOne",
                },
                "moderator": { "login": "mod_login" },
                "message_text": "borderline message",
            }),
        )
    }

    fn config_with_filter(filter: &str) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert(
            "status_filter".to_owned(),
            Variant::String(filter.to_owned()),
        );
        cfg
    }

    #[test]
    fn event_filter_targets_automod_message_update_kind_from_twitch() {
        let filter = AutomodMessageUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.automod.message.update")
        );
    }

    #[test]
    fn status_filter_matches_title_case_payload_case_insensitively() {
        let event = message_event("Approved");
        for (filter, expected) in [
            ("any", true),
            ("approved", true),
            ("denied", false),
            ("expired", false),
        ] {
            let cfg = config_with_filter(filter);
            assert_eq!(
                AutomodMessageUpdatedDescriptor.matches_trigger(&cfg, &event),
                expected,
                "filter {filter:?} against Title-Case status \"Approved\""
            );
        }
    }

    #[test]
    fn missing_status_filter_config_defaults_to_any_and_fires() {
        let event = message_event("Denied");
        let cfg = TriggerConfig::new();
        assert!(AutomodMessageUpdatedDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn build_arg_stack_exposes_status_and_message_id_chaining_vars() {
        let stack = AutomodMessageUpdatedDescriptor.build_arg_stack(&message_event("Approved"));
        assert_eq!(
            stack.get("automod.message_id"),
            Some(&Variant::String("msg-77".to_owned()))
        );
        assert_eq!(
            stack.get("automod.status"),
            Some(&Variant::String("Approved".to_owned()))
        );
        assert_eq!(
            stack.get("user_login"),
            Some(&Variant::String("viewer_one".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_login".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_reads_blocked_term_fields_and_defaults_them_on_classification_updates() {
        let blocked = Event::new(
            EventSource::Twitch,
            "twitch.automod.message.update",
            serde_json::json!({
                "automod": { "message_id": "msg-blocked", "status": "Denied" },
                "reason": "blocked_term",
                "blocked_term": { "terms_found": ["term-1", 42, "term-2"] },
            }),
        );

        for (event, reason, terms) in [
            (blocked, "blocked_term", vec!["term-1", "term-2"]),
            (message_event("Approved"), "", vec![]),
        ] {
            let stack = AutomodMessageUpdatedDescriptor.build_arg_stack(&event);
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
