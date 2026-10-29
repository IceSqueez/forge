use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{ActorRole, ActorSlot, CanonicalVariable, PlatformId, TriggerConfig};

use super::payload_read::{self, youtube_actor};
use crate::payload_fields::gift as fields;

pub(crate) struct ChannelMemberGiftReceivedDescriptor;

impl TriggerKindDescriptor for ChannelMemberGiftReceivedDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member_gift_received"
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
            kind_prefix: Some("youtube.channel.member_gift_received".to_owned()),
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
                    "gift.level_name",
                    "Membership level name",
                    CanonicalVariable::SubTier,
                )
                .legacy(
                    "gifter.channel_id",
                    "Gifter channel ID",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Id),
                )
                .legacy(
                    "gifter.display_name",
                    "Gifter display name",
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Name),
                )
                .legacy(
                    "recipient.channel_id",
                    "Recipient channel ID",
                    CanonicalVariable::actor(ActorRole::Recipient, ActorSlot::Id),
                )
                .legacy(
                    "recipient.display_name",
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
    fn the_legacy_gift_received_names_still_carry_what_their_canonical_twins_carry() {
        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&as_the_poller_sends_it());
        assert_eq!(stack.get("gift.level_name"), stack.get("sub_tier"));
        assert_eq!(stack.get("gifter.channel_id"), stack.get("gifter_id"));
        assert_eq!(stack.get("gifter.display_name"), stack.get("gifter_name"));
        assert_eq!(stack.get("recipient.channel_id"), stack.get("recipient_id"));
        assert_eq!(
            stack.get("recipient.display_name"),
            stack.get("recipient_name")
        );
        assert_eq!(
            stack.get("recipient.channel_id"),
            Some(&Variant::String("UCrecipient".to_owned()))
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
