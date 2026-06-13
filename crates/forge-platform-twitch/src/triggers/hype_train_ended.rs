use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct HypeTrainEndedDescriptor;

impl TriggerKindDescriptor for HypeTrainEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.support.hype_train_ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Bits
    }

    fn label(&self) -> &str {
        "Hype Train ended"
    }

    fn summary(&self) -> &str {
        "Fires when a Hype Train concludes on the channel"
    }

    fn search_text(&self) -> &str {
        "twitch hype train end finish level cooldown"
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
            kind_prefix: Some("channel.hype_train.end".to_owned()),
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
        let total = hype
            .and_then(|h| h.get("total"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let ended_at = hype
            .and_then(|h| h.get("ended_at"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let cooldown_ends_at = hype
            .and_then(|h| h.get("cooldown_ends_at"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        ArgStack::new()
            .set("hype.id".to_owned(), Variant::String(id))
            .set("hype.level".to_owned(), Variant::Int(level))
            .set("hype.total".to_owned(), Variant::Int(total))
            .set("hype.ended_at".to_owned(), Variant::String(ended_at))
            .set(
                "hype.cooldown_ends_at".to_owned(),
                Variant::String(cooldown_ends_at),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end_event() -> Event {
        let payload = serde_json::json!({
            "hype": {
                "id": "ht-9",
                "level": 5,
                "total": 9001,
                "ended_at": "2026-06-13T18:10:00Z",
                "cooldown_ends_at": "2026-06-13T19:10:00Z",
            }
        });
        Event::new(EventSource::Twitch, "channel.hype_train.end", payload)
    }

    #[test]
    fn event_filter_targets_hype_train_end_on_twitch() {
        let filter = HypeTrainEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.hype_train.end")
        );
    }

    #[test]
    fn build_arg_stack_maps_all_vars_with_int_level_and_total() {
        let stack = HypeTrainEndedDescriptor.build_arg_stack(&end_event());
        assert_eq!(
            stack.get("hype.id"),
            Some(&Variant::String("ht-9".to_owned()))
        );
        assert_eq!(stack.get("hype.level"), Some(&Variant::Int(5)));
        assert_eq!(stack.get("hype.total"), Some(&Variant::Int(9001)));
        assert_eq!(
            stack.get("hype.ended_at"),
            Some(&Variant::String("2026-06-13T18:10:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("hype.cooldown_ends_at"),
            Some(&Variant::String("2026-06-13T19:10:00Z".to_owned()))
        );
    }
}
