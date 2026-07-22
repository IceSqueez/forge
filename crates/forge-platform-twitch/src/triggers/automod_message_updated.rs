use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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
            kind_prefix: Some("channel.automod.message.update".to_owned()),
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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let automod = event.payload.get(automod_fields::AUTOMOD);
        let user = event.payload.get(automod_fields::USER);
        let moderator = event.payload.get(automod_fields::MODERATOR);

        let message_id = automod
            .and_then(|a| a.get(automod_fields::MESSAGE_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = automod
            .and_then(|a| a.get(automod_fields::STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let user_login = user
            .and_then(|u| u.get(automod_fields::USER_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let message_text = event
            .payload
            .get(automod_fields::MESSAGE_TEXT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let moderator_login = moderator
            .and_then(|m| m.get(automod_fields::MODERATOR_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("automod.message_id".to_owned(), Variant::String(message_id))
            .set("automod.status".to_owned(), Variant::String(status))
            .set("user_login".to_owned(), Variant::String(user_login))
            .set("message_text".to_owned(), Variant::String(message_text))
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "automod.message_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod message ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "automod.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod status".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "user_login".to_owned(),
                        kind: VariantKind::String,
                        label: "User login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "message_text".to_owned(),
                        kind: VariantKind::String,
                        label: "Message text".to_owned(),
                        synthesis: Some(SynthesisHint::Message),
                    },
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                ],
            }
        })
    }
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
            Some("channel.automod.message.update")
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
}
