use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::prediction as fields;

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
            kind_prefix: Some("twitch.channel.prediction.end".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let prediction = event.payload.get(fields::PREDICTION);

        let prediction_id = prediction
            .and_then(|v| v.get(fields::PREDICTION_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let title = prediction
            .and_then(|v| v.get(fields::PREDICTION_TITLE))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let winning_outcome_id = prediction
            .and_then(|v| v.get(fields::WINNING_OUTCOME_ID))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let status = prediction
            .and_then(|v| v.get(fields::STATUS))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let ended_at = prediction
            .and_then(|v| v.get(fields::ENDED_AT))
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
                    DeclaredVariable {
                        name: "prediction.winning_outcome_id".to_owned(),
                        kind: VariantKind::String,
                        label: "Winning outcome ID".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.status".to_owned(),
                        kind: VariantKind::String,
                        label: "Prediction status".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.ended_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Ended at".to_owned(),
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

    fn prediction_end_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-4",
                "title": "Who wins?",
                "winning_outcome_id": "outcome-42",
                "status": "resolved",
                "ended_at": "2026-06-13T18:10:00Z",
            },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.end",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_prediction_end_topic_from_twitch() {
        let filter = PredictionEndedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.end")
        );
    }

    #[test]
    fn build_arg_stack_carries_winning_outcome_and_status_fields() {
        let stack = PredictionEndedDescriptor.build_arg_stack(&prediction_end_event());
        assert_eq!(
            stack.get("prediction.winning_outcome_id"),
            Some(&Variant::String("outcome-42".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.status"),
            Some(&Variant::String("resolved".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.ended_at"),
            Some(&Variant::String("2026-06-13T18:10:00Z".to_owned()))
        );
    }
}
