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
            kind_prefix: Some("twitch.automod.settings.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(
                    twitch_actor(ActorRole::Principal),
                    settings_moderator_identity,
                )
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    settings_moderator_identity,
                )
                .event_specific(
                    DeclaredVariable {
                        name: "automod.overall_level".to_owned(),
                        kind: VariantKind::Int,
                        label: "Automod overall level".to_owned(),
                        synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 4 }),
                    },
                    |event| {
                        Variant::Int(payload_read::number(event, automod_fields::OVERALL_LEVEL))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn settings_moderator_identity(event: &Event) -> ActorIdentity {
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
            Some("twitch.automod.settings.update")
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
