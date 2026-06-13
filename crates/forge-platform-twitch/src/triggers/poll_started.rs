use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct PollStartedDescriptor;

impl TriggerKindDescriptor for PollStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.poll.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Polls
    }

    fn label(&self) -> &str {
        "Poll started"
    }

    fn summary(&self) -> &str {
        "Fires when a poll begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch poll started begin vote"
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
            kind_prefix: Some("channel.poll.begin".to_owned()),
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
        let started_at = poll
            .and_then(|v| v.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ends_at = poll
            .and_then(|v| v.get("ends_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("poll.id".to_owned(), Variant::String(poll_id))
            .set("poll.title".to_owned(), Variant::String(title))
            .set("poll.started_at".to_owned(), Variant::String(started_at))
            .set("poll.ends_at".to_owned(), Variant::String(ends_at))
    }
}
