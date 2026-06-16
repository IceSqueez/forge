use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChannelBroadcastTitleChangedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastTitleChangedDescriptor {
    fn id(&self) -> &str {
        "youtube.stream.title_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Live broadcast title changed"
    }

    fn summary(&self) -> &str {
        "Fires when the title of an active YouTube live broadcast is edited"
    }

    fn search_text(&self) -> &str {
        "youtube live stream title changed renamed broadcast edit"
    }

    fn icon_name(&self) -> &str {
        "edit"
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
            kind_prefix: Some("youtube.stream.title_changed".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let title_old = event
            .payload
            .get("stream.title_old")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title_new = event
            .payload
            .get("stream.title_new")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("stream.title_old".to_owned(), Variant::String(title_old))
            .set("stream.title_new".to_owned(), Variant::String(title_new))
    }
}
