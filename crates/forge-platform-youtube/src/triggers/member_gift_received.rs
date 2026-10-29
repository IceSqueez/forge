use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, ActorSlot, CanonicalVariable, PlatformId, TriggerConfig};

use super::payload_read::{self, youtube_actor};
use crate::vocabulary::fields::gift as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ChannelMemberGiftReceivedDescriptor;

impl TriggerKindDescriptor for ChannelMemberGiftReceivedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CHANNEL_MEMBER_GIFT_RECEIVED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "Gift membership received"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer receives a gifted YouTube channel membership"
    }

    fn search_text(&self) -> &str {
        "youtube member gift received recipient memberships sponsor subscription level"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::YouTube)
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
            source: Some(EventSource::YouTube),
            kind_prefix: Some(event_kinds::CHANNEL_MEMBER_GIFT_RECEIVED.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(
            TriggerVariables::new()
                .actor(youtube_actor(ActorRole::Principal), recipient_identity)
                .actor(youtube_actor(ActorRole::Recipient), recipient_identity)
                .actor(youtube_actor(ActorRole::Gifter), gifter_identity)
                .sub_tier(|event| payload_read::text(event, fields::LEVEL_NAME))
                .legacy(
                    variables::gift::LEVEL_NAME,
                    "Membership level name",
                    CanonicalVariable::SubTier,
                )
                .legacy(
                    variables::gift::GIFTER_CHANNEL_ID,
                    "Gifter channel ID",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Id),
                )
                .legacy(
                    variables::gift::GIFTER_DISPLAY_NAME,
                    "Gifter display name",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Name),
                )
                .legacy(
                    variables::gift::RECIPIENT_CHANNEL_ID,
                    "Recipient channel ID",
                    CanonicalVariable::actor(ActorRole::Recipient, ActorSlot::Id),
                )
                .legacy(
                    variables::gift::RECIPIENT_DISPLAY_NAME,
                    "Recipient display name",
                    CanonicalVariable::actor(ActorRole::Recipient, ActorSlot::Name),
                ),
        )
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actors(&[ActorRole::Gifter, ActorRole::Recipient])
    }
}

fn gifter_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::GIFTER))
}

fn recipient_identity(event: &Event) -> ActorIdentity {
    payload_read::identity(event.payload.get(fields::RECIPIENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::Variant;
    use serde_json::json;

    fn received_event(payload: serde_json::Value) -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.channel.member_gift_received",
            payload,
        )
    }

    fn as_the_poller_sends_it() -> Event {
        received_event(json!({
            "level_name": "Gold",
            "recipient": { "channel_id": "UCrecipient", "display_name": "LuckyViewer" },
            "gifter": { "channel_id": "UCgifter", "display_name": null },
        }))
    }

    #[test]
    fn the_recipient_is_published_as_both_the_principal_and_the_recipient_role() {
        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&as_the_poller_sends_it());
        for (name, value) in [
            ("user_id", "UCrecipient"),
            ("user_name", "LuckyViewer"),
            ("recipient_id", "UCrecipient"),
            ("recipient_name", "LuckyViewer"),
            ("sub_tier", "Gold"),
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(value.to_owned())),
                "'{name}'"
            );
        }
    }

    #[test]
    fn the_gifter_keeps_the_id_the_wire_names_and_the_display_name_it_never_sends_stays_empty() {
        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&as_the_poller_sends_it());
        assert_eq!(
            stack.get("gifter_id"),
            Some(&Variant::String("UCgifter".to_owned()))
        );
        assert_eq!(
            stack.get("gifter_name"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn a_gift_the_wire_leaves_blank_names_neither_side() {
        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&received_event(json!({})));
        for name in [
            "user_id",
            "user_name",
            "recipient_id",
            "recipient_name",
            "gifter_id",
            "gifter_name",
            "sub_tier",
        ] {
            assert_eq!(
                stack.get(name),
                Some(&Variant::String(String::new())),
                "'{name}'"
            );
        }
    }
}
