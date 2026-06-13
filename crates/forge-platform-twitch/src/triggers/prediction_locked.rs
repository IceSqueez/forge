use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct PredictionLockedDescriptor;

impl TriggerKindDescriptor for PredictionLockedDescriptor {
    fn id(&self) -> &str {
        "twitch.prediction.locked"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Predictions
    }

    fn label(&self) -> &str {
        "Prediction locked"
    }

    fn summary(&self) -> &str {
        "Fires when a prediction is locked and no more points can be placed"
    }

    fn search_text(&self) -> &str {
        "twitch prediction locked closed outcome"
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
            kind_prefix: Some("channel.prediction.lock".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let prediction = event.payload.get("prediction");

        let prediction_id = prediction
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = prediction
            .and_then(|v| v.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let locked_at = prediction
            .and_then(|v| v.get("locked_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
            .set(
                "prediction.locked_at".to_owned(),
                Variant::String(locked_at),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prediction_lock_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-3",
                "title": "Final score?",
                "locked_at": "2026-06-13T18:05:00Z",
            },
        });
        Event::new(EventSource::Twitch, "channel.prediction.lock", payload)
    }

    #[test]
    fn event_filter_targets_prediction_lock_topic_from_twitch() {
        let filter = PredictionLockedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.prediction.lock")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_title_and_locked_at() {
        let stack = PredictionLockedDescriptor.build_arg_stack(&prediction_lock_event());
        assert_eq!(
            stack.get("prediction.id"),
            Some(&Variant::String("pred-3".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.title"),
            Some(&Variant::String("Final score?".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.locked_at"),
            Some(&Variant::String("2026-06-13T18:05:00Z".to_owned()))
        );
    }
}
