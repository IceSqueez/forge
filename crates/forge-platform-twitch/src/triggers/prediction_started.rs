use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct PredictionStartedDescriptor;

impl TriggerKindDescriptor for PredictionStartedDescriptor {
    fn id(&self) -> &str {
        "twitch.prediction.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Predictions
    }

    fn label(&self) -> &str {
        "Prediction started"
    }

    fn summary(&self) -> &str {
        "Fires when a prediction begins on the broadcaster's channel"
    }

    fn search_text(&self) -> &str {
        "twitch prediction started begin outcome"
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
            kind_prefix: Some("channel.prediction.begin".to_owned()),
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
        let started_at = prediction
            .and_then(|v| v.get("started_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let locks_at = prediction
            .and_then(|v| v.get("locks_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
            .set(
                "prediction.started_at".to_owned(),
                Variant::String(started_at),
            )
            .set("prediction.locks_at".to_owned(), Variant::String(locks_at))
    }
}
