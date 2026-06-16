use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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
}
