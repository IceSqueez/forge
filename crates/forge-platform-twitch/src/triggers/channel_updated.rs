use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChannelUpdatedDescriptor;

impl TriggerKindDescriptor for ChannelUpdatedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.update"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Channel updated"
    }

    fn summary(&self) -> &str {
        "Fires when the broadcaster updates their channel title, category, or language"
    }

    fn search_text(&self) -> &str {
        "twitch channel update title category game language"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
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
            kind_prefix: Some("channel.update".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let channel = event.payload.get("channel");

        let title = channel
            .and_then(|c| c.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category_id = channel
            .and_then(|c| c.get("category_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let category_name = channel
            .and_then(|c| c.get("category_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let language = channel
            .and_then(|c| c.get("language"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("channel.title".to_owned(), Variant::String(title))
            .set(
                "channel.category_id".to_owned(),
                Variant::String(category_id),
            )
            .set(
                "channel.category_name".to_owned(),
                Variant::String(category_name),
            )
            .set("channel.language".to_owned(), Variant::String(language))
    }
}
