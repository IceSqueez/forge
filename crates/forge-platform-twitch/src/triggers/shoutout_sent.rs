use super::spec::TwitchTriggerSpec;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalCount, CanonicalVariable,
    TriggerCategory, TriggerVariables,
};
use forge_types::{DeclaredVariable, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::shoutout as shoutout_fields;

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(
                twitch_actor(ActorRole::Principal),
                shoutout_channel_identity,
            )
            .count(CanonicalCount::ViewerCount, |event| {
                payload_read::number(event, shoutout_fields::VIEWER_COUNT)
            })
            .event_specific_optional(
                DeclaredVariable {
                    name: "started_at".to_owned(),
                    kind: VariantKind::Datetime,
                    synthesis: None,
                },
                |event| payload_read::timestamp(event, shoutout_fields::STARTED_AT),
            )
            .legacy(
                "to_broadcaster_login",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
            )
            .legacy(
                "to_broadcaster_id",
                CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
            ),
    )
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::CHANNEL_SHOUTOUT_SENT,
    category: TriggerCategory::Moderation,
    label: "Shoutout sent",
    summary: "Fires when the broadcaster or a moderator gives a shoutout to another channel",
    search_text: "twitch shoutout sent give raid moderation",
    icon: "megaphone",
    kind: event_kinds::CHANNEL_SHOUTOUT_CREATE,
    actors: ActorDeclaration::principal(),
    variables,
    ..TwitchTriggerSpec::BASE
};

fn shoutout_channel_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(shoutout_fields::TO_BROADCASTER),
        shoutout_fields::BROADCASTER_ID,
        shoutout_fields::BROADCASTER_LOGIN,
        shoutout_fields::BROADCASTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn shoutout_sent_event() -> Event {
        let payload = serde_json::json!({
            "to_broadcaster": { "id": "555", "login": "other_chan", "display_name": "OtherChan" },
            "viewer_count": 42,
            "started_at": "2026-06-13T18:00:00Z",
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.shoutout.create",
            payload,
        )
    }

    #[test]
    fn build_arg_stack_maps_target_fields_and_types_viewer_count_as_int() {
        let stack = SPEC.build_arg_stack(&shoutout_sent_event());
        assert_eq!(
            stack.get("to_broadcaster_login"),
            Some(&Variant::String("other_chan".to_owned()))
        );
        assert_eq!(
            stack.get("to_broadcaster_id"),
            Some(&Variant::String("555".to_owned()))
        );
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(42)));
        assert_eq!(
            stack.get("started_at"),
            Some(&instant("2026-06-13T18:00:00Z"))
        );
    }
}
