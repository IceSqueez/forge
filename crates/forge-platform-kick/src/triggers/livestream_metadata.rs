use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct LivestreamMetadataDescriptor;

impl TriggerKindDescriptor for LivestreamMetadataDescriptor {
    fn id(&self) -> &str {
        "kick.channel.livestream_metadata"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Livestream metadata"
    }

    fn summary(&self) -> &str {
        "Fires when the Kick channel stream title or category changes"
    }

    fn search_text(&self) -> &str {
        "kick livestream metadata title category edit update stream"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Kick)
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
            source: Some(EventSource::Kick),
            kind_prefix: Some("kick.channel.livestream_metadata".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let stream_title = event
            .payload
            .get("stream_title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let category = event.payload.get("category");
        let category_id = category
            .and_then(|c| c.get("id"))
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());
        let category_name = category
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("stream_title".to_owned(), Variant::String(stream_title))
            .set("category_id".to_owned(), Variant::String(category_id))
            .set("category_name".to_owned(), Variant::String(category_name))
    }
}
