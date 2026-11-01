use super::spec::TwitchTriggerSpec;
use crate::vocabulary::RaidDirection;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::Event;
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalCount, CanonicalVariable,
    TriggerCategory, TriggerVariables,
};
use forge_types::TriggerConfig;

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::raid as fields;

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    event
        .payload
        .get(fields::DIRECTION)
        .and_then(|v| v.as_str())
        .is_some_and(|d| d == RaidDirection::Sent.as_str())
}

fn variables() -> Option<TriggerVariables> {
    Some(
        TriggerVariables::new()
            .actor(twitch_actor(ActorRole::Principal), raid_target_identity)
            .count(CanonicalCount::ViewerCount, |event| {
                payload_read::number(event, fields::VIEWER_COUNT)
            })
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
    id: trigger_ids::CHANNEL_RAID_SENT,
    category: TriggerCategory::Raids,
    label: "Raid sent",
    summary: "Fires when you raid another streamer's channel",
    search_text: "twitch raid outgoing send host viewers streamer",
    icon: "sword",
    kind: event_kinds::CHANNEL_RAID,
    actors: ActorDeclaration::principal(),
    variables,
    matches,
    ..TwitchTriggerSpec::BASE
};

fn raid_target_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(
        event.payload.get(fields::TO_BROADCASTER),
        fields::BROADCASTER_ID,
        fields::BROADCASTER_LOGIN,
        fields::BROADCASTER_DISPLAY_NAME,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    #[test]
    fn event_filter_targets_channel_raid_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(filter.kind_prefix.as_deref(), Some("twitch.channel.raid"));
    }

    #[test]
    fn build_arg_stack_exposes_to_broadcaster_and_viewer_count_as_int() {
        let event = Event::new(
            EventSource::Twitch,
            "channel.raid",
            serde_json::json!({
                "direction": "sent",
                "viewer_count": 37,
                "to_broadcaster": {
                    "id": "999",
                    "login": "target_chan",
                    "display_name": "TargetChan"
                }
            }),
        );
        let stack = SPEC.build_arg_stack(&event);
        assert_eq!(
            stack.get("to_broadcaster_login"),
            Some(&Variant::String("target_chan".to_owned()))
        );
        assert_eq!(
            stack.get("to_broadcaster_id"),
            Some(&Variant::String("999".to_owned()))
        );
        assert_eq!(stack.get("viewer_count"), Some(&Variant::Int(37)));
    }
}
