use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct PollEndedDescriptor;

impl TriggerKindDescriptor for PollEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.poll.ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Polls
    }

    fn label(&self) -> &str {
        "Poll ended"
    }

    fn summary(&self) -> &str {
        "Fires when a poll ends (completed, archived, or terminated)"
    }

    fn search_text(&self) -> &str {
        "twitch poll ended completed archived terminated"
    }

    fn icon_name(&self) -> &str {
        "chart-bar"
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
            kind_prefix: Some("channel.poll.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let poll = event.payload.get("poll");

        let poll_id = poll
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = poll
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = poll
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = poll
            .and_then(|v| v.get("ended_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("poll.id".to_owned(), Variant::String(poll_id))
            .set("poll.title".to_owned(), Variant::String(title))
            .set("poll.status".to_owned(), Variant::String(status))
            .set("poll.ended_at".to_owned(), Variant::String(ended_at))
    }
}
