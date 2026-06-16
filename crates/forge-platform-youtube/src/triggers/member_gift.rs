use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

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
            .get("gift.count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let level_name = event
            .payload
            .get("gift.level_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_channel_id = event
            .payload
            .get("gifter.channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let gifter_display_name = event
            .payload
            .get("gifter.display_name")
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
}
