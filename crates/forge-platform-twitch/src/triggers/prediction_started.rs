use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::prediction as fields;

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
            kind_prefix: Some("twitch.channel.prediction.begin".to_owned()),
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
        let started_at = prediction
            .and_then(|v| v.get(fields::STARTED_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let locks_at = prediction
            .and_then(|v| v.get(fields::LOCKS_AT))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let outcomes = build_outcomes_variant(event);

        ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
            .set(
                "prediction.started_at".to_owned(),
                Variant::String(started_at),
            )
            .set("prediction.locks_at".to_owned(), Variant::String(locks_at))
            .set("prediction.outcomes".to_owned(), outcomes)
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
                        name: "prediction.started_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.locks_at".to_owned(),
                        kind: VariantKind::String,
                        label: "Locks at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.outcomes".to_owned(),
                        kind: VariantKind::Array,
                        label: "Prediction outcomes".to_owned(),
                        synthesis: None,
                    },
                ],
            }
        })
    }
}

pub(crate) fn build_outcomes_variant(event: &Event) -> Variant {
    let outcomes = event
        .payload
        .get(fields::OUTCOMES)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Variant::Array(
        outcomes
            .iter()
            .map(|outcome| {
                let mut obj = std::collections::BTreeMap::new();
                obj.insert(
                    "id".to_owned(),
                    Variant::String(
                        outcome
                            .get(fields::OUTCOME_ID)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "title".to_owned(),
                    Variant::String(
                        outcome
                            .get(fields::OUTCOME_TITLE)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "color".to_owned(),
                    Variant::String(
                        outcome
                            .get(fields::OUTCOME_COLOR)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "users".to_owned(),
                    Variant::Int(
                        outcome
                            .get(fields::OUTCOME_USERS)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                obj.insert(
                    "channel_points".to_owned(),
                    Variant::Int(
                        outcome
                            .get(fields::OUTCOME_CHANNEL_POINTS)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                Variant::Object(obj)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prediction_begin_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-1",
                "title": "Will we win?",
                "started_at": "2026-06-13T18:00:00Z",
                "locks_at": "2026-06-13T18:02:00Z",
            },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.begin",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_prediction_begin_topic_from_twitch() {
        let filter = PredictionStartedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_title_and_timing_fields() {
        let stack = PredictionStartedDescriptor.build_arg_stack(&prediction_begin_event());
        assert_eq!(
            stack.get("prediction.id"),
            Some(&Variant::String("pred-1".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.title"),
            Some(&Variant::String("Will we win?".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.started_at"),
            Some(&Variant::String("2026-06-13T18:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.locks_at"),
            Some(&Variant::String("2026-06-13T18:02:00Z".to_owned()))
        );
    }
}
