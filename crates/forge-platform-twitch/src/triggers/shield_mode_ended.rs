use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::shield as shield_fields;

fn variables() -> Option<TriggerVariables> {
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
            .event_specific_optional(
                DeclaredVariable {
                    name: "ended_at".to_owned(),
                    kind: VariantKind::Datetime,
                    label: "Ended at".to_owned(),
                    synthesis: None,
                },
                |event| payload_read::timestamp(event, shield_fields::ENDED_AT),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_SHIELD_MODE_ENDED,
    category: TriggerCategory::Moderation,
    label: "Shield mode ended",
    summary: "Fires when a moderator deactivates shield mode in the channel",
    search_text: "twitch shield mode protection moderation ended deactivated",
    icon: "shield-off",
    kind: event_kinds::CHANNEL_SHIELD_MODE_END,
    actors: ActorDeclaration::Actors(&[ActorRole::Moderator]),
    variables,
    ..TwitchTriggerSpec::BASE
};

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
    use crate::timestamp::tests::instant;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

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
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.shield_mode.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_moderator_fields_and_ended_at() {
        let stack = SPEC.build_arg_stack(&shield_end_event());
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
            Some(&instant("2026-06-13T19:00:00Z"))
        );
    }
}
