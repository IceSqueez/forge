use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct HypeTrainStartedDescriptor;

impl TriggerKindDescriptor for HypeTrainStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.support.hype_train_started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Hype
    }

    fn label(&self) -> &str {
        "Hype Train started"
    }

    fn summary(&self) -> &str {
        "Fires when a Hype Train begins on the channel"
    }

    fn search_text(&self) -> &str {
        "twitch hype train begin start level"
    }

    fn icon_name(&self) -> &str {
        "train"
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
            kind_prefix: Some("channel.hype_train.begin".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let hype = event.payload.get("hype");

        let id = hype
            .and_then(|h| h.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let level = hype
            .and_then(|h| h.get("level"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let goal = hype
            .and_then(|h| h.get("goal"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let progress = hype
            .and_then(|h| h.get("progress"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let total = hype
            .and_then(|h| h.get("total"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let started_at = hype
            .and_then(|h| h.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let expires_at = hype
            .and_then(|h| h.get("expires_at"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        ArgStack::new()
            .set("hype.id".to_owned(), Variant::String(id))
            .set("hype.level".to_owned(), Variant::Int(level))
            .set("hype.goal".to_owned(), Variant::Int(goal))
            .set("hype.progress".to_owned(), Variant::Int(progress))
            .set("hype.total".to_owned(), Variant::Int(total))
            .set("hype.started_at".to_owned(), Variant::String(started_at))
            .set("hype.expires_at".to_owned(), Variant::String(expires_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn begin_event() -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-1",
                "level": 3,
                "goal": 1000,
                "progress": 450,
                "total": 450,
                "started_at": "2026-06-13T18:00:00Z",
                "expires_at": "2026-06-13T18:05:00Z",
            }
        });
        Event::new(EventSource::Twitch, "channel.hype_train.begin", payload)
    }

    #[test]
    fn event_filter_targets_hype_train_begin_on_twitch() {
        let filter = HypeTrainStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.hype_train.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_progress_fields() {
        let stack = HypeTrainStartedDescriptor.build_arg_stack(&begin_event());
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-1".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(3)));
        assert_eq!(stack.get("hype.goal"), Some(&Variant::Int(1000)));
        assert_eq!(stack.get("hype.progress"), Some(&Variant::Int(450)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(450)));
        assert_eq!(
            stack.get("hype.started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("hype.expires_at"),
            Some(&Variant::String("2026-06-13T18:05:00Z".to_owned()))
        );
    }
}
