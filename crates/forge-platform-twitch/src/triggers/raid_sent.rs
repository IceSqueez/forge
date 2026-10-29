use crate::vocabulary::RaidDirection;
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalCount, CanonicalVariable, PlatformId, TriggerConfig,
};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::raid as fields;

pub(crate) struct RaidSentDescriptor;

impl TriggerKindDescriptor for RaidSentDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_RAID_SENT
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Raids
    }

    fn label(&self) -> &str {
        "Raid sent"
    }

    fn summary(&self) -> &str {
        "Fires when you raid another streamer's channel"
    }

    fn search_text(&self) -> &str {
        "twitch raid outgoing send host viewers streamer"
    }

    fn icon_name(&self) -> &str {
        "sword"
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
            kind_prefix: Some(event_kinds::CHANNEL_RAID.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event
            .payload
            .get(fields::DIRECTION)
            .and_then(|v| v.as_str())
            .is_some_and(|d| d == RaidDirection::Sent.as_str())
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(twitch_actor(ActorRole::Principal), raid_target_identity)
                .count(CanonicalCount::ViewerCount, |event| {
                    payload_read::number(event, fields::VIEWER_COUNT)
                })
                .legacy(
                    "to_broadcaster_login",
                    "Target channel login",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Login),
                )
                .legacy(
                    "to_broadcaster_id",
                    "Target channel ID",
                    CanonicalVariable::actor(ActorRole::Principal, ActorSlot::Id),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::principal()
    }
}

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
    use forge_types::Variant;

    #[test]
    fn event_filter_targets_channel_raid_topic_from_twitch() {
        let filter = RaidSentDescriptor.event_filter();
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
        let stack = RaidSentDescriptor.build_arg_stack(&event);
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
