use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, ActorRole, ActorSlot, CanonicalCount, CanonicalVariable,
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    TriggerVariables,
};
use forge_types::{DeclaredVariable, PlatformId, TriggerConfig, VariantKind};

use super::payload_read::{self, twitch_actor};
use crate::vocabulary::fields::shoutout as shoutout_fields;

pub(crate) struct ShoutoutSentDescriptor;

impl TriggerKindDescriptor for ShoutoutSentDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_SHOUTOUT_SENT
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shoutout sent"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster or a moderator gives a shoutout to another channel"
    }

    fn search_text(&self) -> &str {
        "twitch shoutout sent give raid moderation"
    }

    fn icon_name(&self) -> &str {
        "megaphone"
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_SHOUTOUT_CREATE.to_owned()),
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
                    shoutout_channel_identity,
                )
                .count(CanonicalCount::ViewerCount, |event| {
                    payload_read::number(event, shoutout_fields::VIEWER_COUNT)
                })
                .event_specific_optional(
                    DeclaredVariable {
                        name: "started_at".to_owned(),
                        kind: VariantKind::Datetime,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    |event| payload_read::timestamp(event, shoutout_fields::STARTED_AT),
                )
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
    fn event_filter_targets_shoutout_create_topic_from_twitch() {
        let filter = ShoutoutSentDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.shoutout.create")
        );
    }

    #[test]
    fn build_arg_stack_maps_target_fields_and_types_viewer_count_as_int() {
        let stack = ShoutoutSentDescriptor.build_arg_stack(&shoutout_sent_event());
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
