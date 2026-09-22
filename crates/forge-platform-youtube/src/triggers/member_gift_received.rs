use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, ActorIdentity, EventFilter, FormField, KindPlatformContract, TriggerCategory,
    TriggerKindDescriptor, TriggerVariables,
};
use forge_types::{
    ActorRole, ActorSlot, CanonicalVariable, DeclaredVariable, PlatformId, SynthesisHint,
    TriggerConfig, Variant, VariantKind,
};

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
                    DeclaredVariable {
                        name: "gift.level_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Membership level name".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::SubTier,
                    |event| Variant::String(payload_read::text(event, fields::LEVEL_NAME)),
                )
                .legacy(
                    DeclaredVariable {
                        name: "gifter.channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Id),
                    |event| Variant::String(gifter_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "gifter.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Gifter display name".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Gifter, ActorSlot::Name),
                    |event| Variant::String(gifter_identity(event).display_name),
                )
                .legacy(
                    DeclaredVariable {
                        name: "recipient.channel_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Recipient channel ID".to_owned(),
                        synthesis: None,
                    },
                    CanonicalVariable::actor(ActorRole::Recipient, ActorSlot::Id),
                    |event| Variant::String(recipient_identity(event).id),
                )
                .legacy(
                    DeclaredVariable {
                        name: "recipient.display_name".to_owned(),
                        kind: VariantKind::String,
                        label: "Recipient display name".to_owned(),
                        synthesis: Some(SynthesisHint::DisplayName),
                    },
                    CanonicalVariable::actor(ActorRole::Recipient, ActorSlot::Name),
                    |event| Variant::String(recipient_identity(event).display_name),
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
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn build_arg_stack_surfaces_level_recipient_and_absent_gifter_as_empty() {
        let event = received_event(json!({
            "level_name": "Gold",
            "recipient": { "channel_id": "UCrecipient", "display_name": "LuckyViewer" },
        }));

        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String("Gold".to_owned()))
        );
        assert_eq!(
            stack.get("recipient.channel_id"),
            Some(&Variant::String("UCrecipient".to_owned()))
        );
        assert_eq!(
            stack.get("recipient.display_name"),
            Some(&Variant::String("LuckyViewer".to_owned()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_defaults_every_key_to_empty() {
        let event = received_event(json!({}));

        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("recipient.channel_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("recipient.display_name"),
            Some(&Variant::String(String::new()))
        );
    }
}
