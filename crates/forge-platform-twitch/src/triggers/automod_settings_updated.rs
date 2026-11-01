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
                |event| Variant::Int(payload_read::number(event, automod_fields::OVERALL_LEVEL)),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::AUTOMOD_SETTINGS_UPDATED,
    category: TriggerCategory::Moderation,
    label: "AutoMod settings updated",
    summary: "Fires when a moderator changes the channel AutoMod filter settings",
    search_text: "twitch automod settings filter level moderator updated changed",
    icon: "shield",
    kind: event_kinds::AUTOMOD_SETTINGS_UPDATE,
    actors: ActorDeclaration::Actors(&[ActorRole::Moderator]),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;

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
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.automod.settings.update")
        );
    }

    #[test]
    fn build_arg_stack_marshals_overall_level_as_int_and_exposes_moderator() {
        let stack = SPEC.build_arg_stack(&settings_event());
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
