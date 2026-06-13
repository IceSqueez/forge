use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct ShoutoutReceivedDescriptor;

impl TriggerKindDescriptor for ShoutoutReceivedDescriptor {
    fn id(&self) -> &str {
        "twitch.channel.shoutout_received"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Moderation
    }

    fn label(&self) -> &str {
        "Shoutout received"
    }

    fn summary(&self) -> &str {
        "Fires when another broadcaster gives the channel a shoutout"
    }

    fn search_text(&self) -> &str {
        "twitch shoutout received incoming raid moderation"
    }

    fn icon_name(&self) -> &str {
        "megaphone"
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
            kind_prefix: Some("channel.shoutout.receive".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let from = event.payload.get("from_broadcaster");

        let from_broadcaster_login = from
            .and_then(|v| v.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let from_broadcaster_id = from
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let viewer_count = event
            .payload
            .get("viewer_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let started_at = event
            .payload
            .get("started_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "from_broadcaster_login".to_owned(),
                Variant::String(from_broadcaster_login),
            )
            .set(
                "from_broadcaster_id".to_owned(),
                Variant::String(from_broadcaster_id),
            )
            .set("viewer_count".to_owned(), Variant::Int(viewer_count))
            .set("started_at".to_owned(), Variant::String(started_at))
    }
}
