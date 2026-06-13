use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct AdBreakStartedDescriptor;

impl TriggerKindDescriptor for AdBreakStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.stream.ad_break_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Streams
    }

    fn label(&self) -> &str {
        "Ad break started"
    }

    fn summary(&self) -> &str {
        "Fires when an ad break begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch ad break commercial started begun automatic"
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
            kind_prefix: Some("channel.ad_break.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let ad_break = event.payload.get("ad_break");
        let requester = event.payload.get("requester");

        let duration_seconds = ad_break
            .and_then(|a| a.get("duration_seconds"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let is_automatic = ad_break
            .and_then(|a| a.get("is_automatic"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let started_at = ad_break
            .and_then(|a| a.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let requester_login = requester
            .and_then(|r| r.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set(
                "ad_break.duration_seconds".to_owned(),
                Variant::Int(duration_seconds),
            )
            .set(
                "ad_break.is_automatic".to_owned(),
                Variant::Bool(is_automatic),
            )
            .set(
                "ad_break.started_at".to_owned(),
                Variant::String(started_at),
            )
            .set(
                "requester_login".to_owned(),
                Variant::String(requester_login),
            )
    }
}
