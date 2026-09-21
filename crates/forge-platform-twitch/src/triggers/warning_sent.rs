use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::warning as warning_fields;

pub(crate) struct WarningSentDescriptor;

impl TriggerKindDescriptor for WarningSentDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.warning_sent"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Warning sent"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator issues a warning to a user"
    }

    fn search_text(&self) -> &str {
        "twitch warning sent moderator reason chat rules moderation"
    }

    fn icon_name(&self) -> &str {
        "bell-alert"
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
            kind_prefix: Some("twitch.channel.warning.send".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), warned_user_identity)
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    warning_moderator_identity,
                )
                .event_specific(
                    DeclaredVariable {
                        name: "warning.reason".to_owned(),
                        kind: VariantKind::String,
                        label: "Warning reason".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, warning_fields::REASON)),
                )
                .event_specific(
                    DeclaredVariable {
                        name: "warning.chat_rules_cited".to_owned(),
                        kind: VariantKind::Array,
                        label: "Cited chat rules".to_owned(),
                        synthesis: None,
                    },
                    |event| payload_read::text_list(event, warning_fields::CHAT_RULES_CITED),
                )
                .legacy(
                    DeclaredVariable {
                        name: "warning.target.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Warned user login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            warning_fields::USER,
                            warning_fields::USER_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "warning.target.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Warned user ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            warning_fields::USER,
                            warning_fields::USER_ID,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "warning.target.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Warned user display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Name),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            warning_fields::USER,
                            warning_fields::USER_DISPLAY_NAME,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "warning.moderator.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            warning_fields::MODERATOR,
                            warning_fields::MODERATOR_ID,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "warning.moderator.login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            warning_fields::MODERATOR,
                            warning_fields::MODERATOR_LOGIN,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn warned_user_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(warning_fields::USER),
        warning_fields::USER_ID,
        warning_fields::USER_LOGIN,
        warning_fields::USER_DISPLAY_NAME,
    )
}

fn warning_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(warning_fields::MODERATOR),
        warning_fields::MODERATOR_ID,
        warning_fields::MODERATOR_LOGIN,
        warning_fields::MODERATOR_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warning_send_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::Twitch, "twitch.channel.warning.send", payload)
    }

    fn full_payload() -> serde_json::Value {
        serde_json::json!({
            "user": { "id": "999", "login": "rulebreaker", "display_name": "RuleBreaker" },
            "moderator": { "login": "mod_jane" },
            "reason": "spamming links",
            "chat_rules_cited": ["No spam", "Be kind"],
        })
    }

    #[test]
    fn event_filter_targets_warning_send_topic_from_twitch() {
        let filter = WarningSentDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.warning.send")
        );
    }

    #[test]
    fn build_arg_stack_surfaces_target_and_moderator_scalar_fields() {
        let stack = WarningSentDescriptor.build_arg_stack(&warning_send_event(full_payload()));
        assert_eq!(
            stack.get("warning.target.id"),
            Some(&Variant::String("999".to_owned()))
        );
        assert_eq!(
            stack.get("warning.target.login"),
            Some(&Variant::String("rulebreaker".to_owned()))
        );
        assert_eq!(
            stack.get("warning.target.display_name"),
            Some(&Variant::String("RuleBreaker".to_owned()))
        );
        assert_eq!(
            stack.get("warning.moderator.login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("warning.reason"),
            Some(&Variant::String("spamming links".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_marshals_chat_rules_json_array_into_variant_array() {
        let stack = WarningSentDescriptor.build_arg_stack(&warning_send_event(full_payload()));
        assert_eq!(
            stack.get("warning.chat_rules_cited"),
            Some(&Variant::Array(vec![
                Variant::String("No spam".to_owned()),
                Variant::String("Be kind".to_owned()),
            ]))
        );
    }

    #[test]
    fn build_arg_stack_yields_empty_array_when_chat_rules_cited_missing() {
        let payload = serde_json::json!({
            "user": { "id": "1", "login": "u", "display_name": "U" },
            "moderator": { "login": "m" },
            "reason": "r",
        });
        let stack = WarningSentDescriptor.build_arg_stack(&warning_send_event(payload));
        assert_eq!(
            stack.get("warning.chat_rules_cited"),
            Some(&Variant::Array(vec![]))
        );
    }

    #[test]
    fn build_arg_stack_yields_empty_array_when_chat_rules_cited_is_not_an_array() {
        let payload = serde_json::json!({
            "user": { "id": "1", "login": "u", "display_name": "U" },
            "moderator": { "login": "m" },
            "reason": "r",
            "chat_rules_cited": "No spam",
        });
        let stack = WarningSentDescriptor.build_arg_stack(&warning_send_event(payload));
        assert_eq!(
            stack.get("warning.chat_rules_cited"),
            Some(&Variant::Array(vec![]))
        );
    }

    #[test]
    fn build_arg_stack_drops_non_string_chat_rule_entries() {
        let payload = serde_json::json!({
            "user": { "id": "1", "login": "u", "display_name": "U" },
            "moderator": { "login": "m" },
            "reason": "r",
            "chat_rules_cited": ["Keep it civil", 42, { "id": "rule-3" }, "No doxxing"],
        });
        let stack = WarningSentDescriptor.build_arg_stack(&warning_send_event(payload));
        assert_eq!(
            stack.get("warning.chat_rules_cited"),
            Some(&Variant::Array(vec![
                Variant::String("Keep it civil".to_owned()),
                Variant::String("No doxxing".to_owned()),
            ]))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_yields_empty_strings_and_empty_array() {
        let stack =
            WarningSentDescriptor.build_arg_stack(&warning_send_event(serde_json::json!({})));
        assert_eq!(
            stack.get("warning.target.id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("warning.moderator.login"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("warning.chat_rules_cited"),
            Some(&Variant::Array(vec![]))
        );
    }
}
