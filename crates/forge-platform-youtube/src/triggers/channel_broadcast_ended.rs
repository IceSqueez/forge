use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ChannelBroadcastEndedDescriptor;

impl TriggerKindDescriptor for ChannelBroadcastEndedDescriptor {
    fn id(&self) -> &str {
        "youtube.channel.live_broadcast_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Live broadcast ended"
    }

    fn summary(&self) -> &str {
        "Fires when a YouTube live broadcast ends"
    }

    fn search_text(&self) -> &str {
        "youtube live broadcast ended stream offline channel"
    }

    fn icon_name(&self) -> &str {
        "radio-off"
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
            kind_prefix: Some("youtube.channel.live_broadcast_ended".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let broadcast_id = event
            .payload
            .get("broadcast_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new().set("broadcast_id".to_owned(), Variant::String(broadcast_id))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn broadcast_ended_event() -> Event {
        Event::new(
            EventSource::YouTube,
            "youtube.channel.live_broadcast_ended",
            serde_json::json!({
                "broadcast_id": "broadcast_xyz"
            }),
        )
    }

    #[test]
    fn always_matches() {
        assert!(
            ChannelBroadcastEndedDescriptor
                .matches_trigger(&TriggerConfig::new(), &broadcast_ended_event())
        );
    }

    #[test]
    fn build_arg_stack_extracts_broadcast_id() {
        let stack = ChannelBroadcastEndedDescriptor.build_arg_stack(&broadcast_ended_event());
        assert_eq!(
            stack.get("broadcast_id"),
            Some(&Variant::String("broadcast_xyz".to_owned()))
        );
    }
}
