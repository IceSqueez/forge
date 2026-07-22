use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::automod as automod_fields;

pub(crate) struct AutomodSettingsUpdatedDescriptor;

impl TriggerKindDescriptor for AutomodSettingsUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.automod.settings_updated"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "AutoMod settings updated"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator changes the channel AutoMod filter settings"
    }

    fn search_text(&self) -> &str {
        "twitch automod settings filter level moderator updated changed"
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
            kind_prefix: Some("channel.automod.settings.update".to_owned()),
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
        let moderator_id = moderator
            .and_then(|m| m.get(automod_fields::MODERATOR_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let overall_level = event
            .payload
            .get(automod_fields::OVERALL_LEVEL)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        ArgStack::new()
            .set(
                "moderator_login".to_owned(),
                Variant::String(moderator_login),
            )
            .set("moderator_id".to_owned(), Variant::String(moderator_id))
            .set(
                "automod.overall_level".to_owned(),
                Variant::Int(overall_level),
            )
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
                        name: "moderator_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "automod.overall_level".to_owned(),
                        kind: VariantKind::Int,
                        label: "Automod overall level".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 4 }),
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_event() -> Event {
        Event::new(
            EventSource::Twitch,
            "channel.automod.settings.update",
            serde_json::json!({
                "moderator": {
                    "id": "mod-42",
                    "login": "mod_login",
                    "display_name": "ModLogin",
                },
                "overall_level": 3,
            }),
        )
    }

    #[test]
    fn event_filter_targets_automod_settings_update_kind_from_twitch() {
        let filter = AutomodSettingsUpdatedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.automod.settings.update")
        );
    }

    #[test]
    fn build_arg_stack_marshals_overall_level_as_int_and_exposes_moderator() {
        let stack = AutomodSettingsUpdatedDescriptor.build_arg_stack(&settings_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_login".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_id"),
            Some(&Variant::String("mod-42".to_owned()))
        );
        assert_eq!(stack.get("automod.overall_level"), Some(&Variant::Int(3)));
    }
}
