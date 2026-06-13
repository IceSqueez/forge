use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, PlatformId, TriggerConfig, Variant};

pub(crate) struct PredictionEndedDescriptor;

impl TriggerKindDescriptor for PredictionEndedDescriptor {
    fn id(&self) -> &str {
        "twitch.prediction.ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Predictions
    }

    fn label(&self) -> &str {
        "Prediction ended"
    }

    fn summary(&self) -> &str {
        "Fires when a prediction resolves or is canceled"
    }

    fn search_text(&self) -> &str {
        "twitch prediction ended resolved canceled winner outcome"
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
            kind_prefix: Some("channel.prediction.end".to_owned()),
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
        let winning_outcome_id = prediction
            .and_then(|v| v.get("winning_outcome_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = prediction
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = prediction
            .and_then(|v| v.get("ended_at"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
            .set(
                "prediction.winning_outcome_id".to_owned(),
                Variant::String(winning_outcome_id),
            )
            .set("prediction.status".to_owned(), Variant::String(status))
            .set("prediction.ended_at".to_owned(), Variant::String(ended_at))
    }
}
