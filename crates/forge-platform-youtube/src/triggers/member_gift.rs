use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, SynthesisHint, TriggerConfig, VariableSchema, Variant,
    VariantKind,
};

use crate::payload_fields::gift as fields;

pub(crate) struct ChannelMemberGiftDescriptor;

impl TriggerKindDescriptor for ChannelMemberGiftDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.member_gift"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Users
    }

    fn label(&self) -> &str {
        "Memberships gifted"
    }

    fn summary(&self) -> &str {
        "Fires when a viewer gifts a batch of YouTube channel memberships"
    }

    fn search_text(&self) -> &str {
        "youtube member gift gifted memberships sponsor subscription level batch"
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
            kind_prefix: Some("youtube.channel.member_gift".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let count = event
            .payload
            .get(fields::COUNT)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let level_name = event
            .payload
            .get(fields::LEVEL_NAME)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_channel_id = event
            .payload
            .get(fields::GIFTER_CHANNEL_ID)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_display_name = event
            .payload
            .get(fields::GIFTER_DISPLAY_NAME)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("gift.count".to_owned(), Variant::Int(count))
            .set("gift.level_name".to_owned(), Variant::String(level_name))
            .set(
                "gifter.channel_id".to_owned(),
                Variant::String(gifter_channel_id),
            )
            .set(
                "gifter.display_name".to_owned(),
                Variant::String(gifter_display_name),
            )
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "gift.count".to_owned(),
                    kind: VariantKind::Int,
                    label: "Memberships gifted count".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 1, max: 100 }),
                },
                DeclaredVariable {
                    name: "gift.level_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Membership level name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "gifter.channel_id".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifter channel ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "gifter.display_name".to_owned(),
                    kind: VariantKind::String,
                    label: "Gifter display name".to_owned(),
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

    fn gift_event(payload: serde_json::Value) -> Event {
        Event::new(EventSource::YouTube, "youtube.channel.member_gift", payload)
    }

    #[test]
    fn build_arg_stack_surfaces_count_level_and_gifter() {
        let event = gift_event(json!({
            "gift.count": 5_i64,
            "gift.level_name": "Diamond",
            "gifter.channel_id": "UCgifter",
            "gifter.display_name": "Generous",
        }));

        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("gift.count"), Some(&Variant::Int(5)));
        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String("Diamond".to_owned()))
        );
        assert_eq!(
            stack.get("gifter.channel_id"),
            Some(&Variant::String("UCgifter".to_owned()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String("Generous".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_on_empty_payload_defaults_count_to_zero_and_strings_empty() {
        let event = gift_event(json!({}));

        let stack = ChannelMemberGiftDescriptor.build_arg_stack(&event);

        assert_eq!(stack.get("gift.count"), Some(&Variant::Int(0)));
        assert_eq!(
            stack.get("gift.level_name"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("gifter.channel_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(
            stack.get("gifter.display_name"),
            Some(&Variant::String(String::new()))
        );
    }

    #[test]
    fn event_filter_targets_member_gift_kind_on_youtube() {
        let filter = ChannelMemberGiftDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::YouTube));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("youtube.channel.member_gift")
        );
    }
}
