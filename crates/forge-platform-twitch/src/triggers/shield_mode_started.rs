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
use crate::payload_fields::shield as shield_fields;

pub(crate) struct ShieldModeStartedDescriptor;

impl TriggerKindDescriptor for ShieldModeStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shield_mode_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shield mode started"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator activates shield mode in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch shield mode protection moderation started activated"
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
            kind_prefix: Some("twitch.channel.shield_mode.begin".to_owned()),
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
                    shield_moderator_identity,
                )
                .actor(
                    twitch_actor(ActorRole::Moderator),
                    shield_moderator_identity,
                )
                .event_specific(
                    DeclaredVariable {
                        name: "started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, shield_fields::STARTED_AT)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "moderator_login".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator login".to_owned(),
                        synthesis: Some(SynthesisHint::Username),
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Login),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shield_fields::MODERATOR,
                            shield_fields::MODERATOR_LOGIN,
                        ))
                    },
                )
                .legacy(
                    DeclaredVariable {
                        name: "moderator_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Moderator ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Moderator, ActorSlot::Id),
                    |event| {
                        Variant::String(payload_read::nested_text(
                            event,
                            shield_fields::MODERATOR,
                            shield_fields::MODERATOR_ID,
                        ))
                    },
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Moderator])
    }
}

fn shield_moderator_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(shield_fields::MODERATOR),
        shield_fields::MODERATOR_ID,
        shield_fields::MODERATOR_LOGIN,
        shield_fields::MODERATOR_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shield_begin_event() -> Event {
        let payload = serde_json::json!({
            "moderator": { "id": "42", "login": "mod_jane", "display_name": "ModJane" },
            "started_at": "2026-06-13T18:00:00Z",
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.shield_mode.begin",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_shield_mode_begin_topic_from_twitch() {
        let filter = ShieldModeStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.shield_mode.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_moderator_fields_and_started_at() {
        let stack = ShieldModeStartedDescriptor.build_arg_stack(&shield_begin_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
    }
}
