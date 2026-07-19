use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub(crate) struct PredictionProgressDescriptor;

impl TriggerKindDescriptor for PredictionProgressDescriptor {
    fn id(&self) -> &str {
        "twitch.prediction.progress"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Predictions
    }

    fn label(&self) -> &str {
        "Prediction progress"
    }

    fn summary(&self) -> &str {
        "Fires when point totals update on an active prediction"
    }

    fn search_text(&self) -> &str {
        "twitch prediction progress update outcome points"
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
            kind_prefix: Some("channel.prediction.progress".to_owned()),
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

        ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
    }
    fn output_schema(&self) -> Option<VariableSchema> {
        Some({
            VariableSchema {
                variables: vec![
                    DeclaredVariable {
                        name: "prediction.id".to_owned(),
                        kind: VariantKind::String,
                        label: "Prediction ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.title".to_owned(),
                        kind: VariantKind::String,
                        label: "Prediction title".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prediction_progress_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-2",
                "title": "Next round outcome",
            },
        });
        Event::new(EventSource::Twitch, "channel.prediction.progress", payload)
    }

    #[test]
    fn event_filter_targets_prediction_progress_topic_from_twitch() {
        let filter = PredictionProgressDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("channel.prediction.progress")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_and_title_only() {
        let stack = PredictionProgressDescriptor.build_arg_stack(&prediction_progress_event());
        assert_eq!(
            stack.get("prediction.id"),
            Some(&Variant::String("pred-2".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.title"),
            Some(&Variant::String("Next round outcome".to_owned()))
        );
    }
}
