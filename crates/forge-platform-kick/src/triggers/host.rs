use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct HostDescriptor;

impl TriggerKindDescriptor for HostDescriptor {
    fn id(&self) -> &str {
        "kick.host"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Channel hosted"
    }

    fn summary(&self) -> &str {
        "Fires when another streamer hosts this Kick channel"
    }

    fn search_text(&self) -> &str {
        "kick host raid streamer hosting channel"
    }

    fn icon_name(&self) -> &str {
        "users"
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
            kind_prefix: Some("kick.host".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let host_username = event
            .payload
            .get("host_username")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        let viewer_count = event
            .payload
            .get("number_viewers")
            .and_then(|v| v.as_u64())
            .map_or_else(String::new, |n| n.to_string());

        ArgStack::new()
            .set("host_username".to_owned(), Variant::String(host_username))
            .set("viewer_count".to_owned(), Variant::String(viewer_count))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn host_event() -> Event {
        Event::new(
            EventSource::Kick,
            "kick.host",
            serde_json::json!({
                "host_username": "hosting_channel",
                "number_viewers": 250
            }),
        )
    }

    #[test]
    fn kind_id_is_canonical() {
        assert_eq!(HostDescriptor.id(), "kick.host");
    }

    #[test]
    fn category_is_streams() {
        assert_eq!(HostDescriptor.category(), TriggerCategory::Streams);
    }

    #[test]
    fn platform_contract_is_kick() {
        assert_eq!(
            HostDescriptor.platform_contract(),
            KindPlatformContract::PlatformSpecific(PlatformId::Kick)
        );
    }

    #[test]
    fn build_arg_stack_extracts_host_fields() {
        let stack = HostDescriptor.build_arg_stack(&host_event());
        assert_eq!(
            stack.get("host_username"),
            Some(&Variant::String("hosting_channel".to_owned()))
        );
        assert_eq!(
            stack.get("viewer_count"),
            Some(&Variant::String("250".to_owned()))
        );
    }
}
