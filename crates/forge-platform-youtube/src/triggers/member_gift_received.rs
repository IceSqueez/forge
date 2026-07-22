use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

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

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let level_name = event
            .payload
            .get("gift.level_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_display_name = event
            .payload
            .get("gifter.display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let recipient_channel_id = event
            .payload
            .get("recipient.channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let recipient_display_name = event
            .payload
            .get("recipient.display_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("gift.level_name".to_owned(), Variant::String(level_name))
            .set(
                "gifter.display_name".to_owned(),
                Variant::String(gifter_display_name),
            )
            .set(
                "recipient.channel_id".to_owned(),
                Variant::String(recipient_channel_id),
            )
            .set(
                "recipient.display_name".to_owned(),
                Variant::String(recipient_display_name),
            )
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "gift.level_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Membership level name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "gifter.display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifter display name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "recipient.channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Recipient channel ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "recipient.display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Recipient display name".to_owned(),
                    synthesis: Some(SynthesisHint::DisplayName),
                },
            ],
        })
    }
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
    fn build_arg_stack_surfaces_level_and_recipient() {
        let event = received_event(json!({
            "gift.level_name": "Gold",
            "gifter.display_name": "",
            "recipient.channel_id": "UCrecipient",
            "recipient.display_name": "LuckyViewer",
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
    }

    #[test]
    fn build_arg_stack_passes_through_unsourced_gifter_name_as_empty() {
        let event = received_event(json!({
            "gift.level_name": "Gold",
            "gifter.display_name": "",
            "recipient.channel_id": "UCrecipient",
            "recipient.display_name": "LuckyViewer",
        }));

        let stack = ChannelMemberGiftReceivedDescriptor.build_arg_stack(&event);

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

    #[test]
    fn event_filter_targets_member_gift_received_kind_on_youtube() {
        let filter = ChannelMemberGiftReceivedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::YouTube));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("youtube.channel.member_gift_received")
        );
    }
}
