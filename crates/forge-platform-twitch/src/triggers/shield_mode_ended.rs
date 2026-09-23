use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, DeclaredVariable, PlatformId, TriggerConfig, Variant, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::payload_fields::shield as shield_fields;

pub(crate) struct ShieldModeEndedDescriptor;

impl TriggerKindDescriptor for ShieldModeEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shield_mode_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shield mode ended"
    }

    fn summary(&self) -> &str {
        "Fires when a moderator deactivates shield mode in the channel"
    }

    fn search_text(&self) -> &str {
        "twitch shield mode protection moderation ended deactivated"
    }

    fn icon_name(&self) -> &str {
        "shield-off"
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
            kind_prefix: Some("twitch.channel.shield_mode.end".to_owned()),
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
                        name: "ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
                        synthesis: None,
                    },
                    |event| Variant::String(payload_read::text(event, shield_fields::ENDED_AT)),
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

    fn shield_end_event() -> Event {
        let payload = serde_json::json!({
            "moderator": { "id": "42", "login": "mod_jane", "display_name": "ModJane" },
            "ended_at": "2026-06-13T19:00:00Z",
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.shield_mode.end",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_shield_mode_end_topic_from_twitch() {
        let filter = ShieldModeEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.shield_mode.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_moderator_fields_and_ended_at() {
        let stack = ShieldModeEndedDescriptor.build_arg_stack(&shield_end_event());
        assert_eq!(
            stack.get("moderator_login"),
            Some(&Variant::String("mod_jane".to_owned()))
        );
        assert_eq!(
            stack.get("moderator_id"),
            Some(&Variant::String("42".to_owned()))
        );
        assert_eq!(
            stack.get("ended_at"),
            Some(&Variant::String("2026-06-13T19:00:00Z".to_owned()))
        );
    }
}
