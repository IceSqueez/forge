use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::automod as automod_fields;

pub(crate) struct AutomodTermsUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodTermsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.terms_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod terms updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator adds or removes permitted or blocked AutoMod terms"
    }

    fn search_text(&self) -> &str {
        "twitch automod terms blocked permitted moderator added removed updated"
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
            kind_prefix: Some("twitch.automod.terms.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let moderator = event.payload.get(automod_fields::MODERATOR);

        let moderator_login = moderator
            .and_then(|m| m.get(automod_fields::MODERATOR_LOGIN))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let action = event
            .payload
            .get(automod_fields::ACTION)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("automod.action".to_owned(), Variant::String(action))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    DeclaredVariable {
                        name: "automod.action".to_owned(),
                        kind: VariantKind::String,
                        label: "Automod terms action".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.terms.update",
            serde_json::json!({
                "moderator": {
                    "id": "mod-7",
                    "login": "mod_login",
                    "display_name": "ModLogin",
                },
                "action": "add_blocked",
                "terms": ["badword"],
            }),
        )
    }

    #[test]
    fn event_filter_targets_automod_terms_update_kind_from_twitch() {
        let filter = AutomodTermsUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.automod.terms.update")
        );
    }

    #[test]
    fn build_arg_stack_reads_top_level_action_into_automod_action_var() {
        let stack = AutomodTermsUpdatedDescriptor.build_arg_stack(&terms_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_login".to_owned()))
        );
        assert_eq!(
            stack.get("automod.action"),
            Some(&Variant::String("add_blocked".to_owned()))
        );
    }
}
