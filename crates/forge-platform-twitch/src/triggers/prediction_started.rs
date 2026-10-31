use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_events::{Event, EventSource};
use forge_registry::{
    ActorDeclaration, EventFilter, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, PlatformId, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::prediction as fields;

pub(crate) struct PredictionStartedDescriptor;

impl TriggerKindDescriptor for PredictionStartedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::PREDICTION_STARTED
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

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(event_kinds::CHANNEL_PREDICTION_BEGIN.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn actors(&self) -> ActorDeclaration {
        ActorDeclaration::Actorless
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
            .and_then(crate::timestamp::variant);
        let locks_at = prediction
            .and_then(|v| v.get(fields::LOCKS_AT))
            .and_then(|v| v.as_str())
            .and_then(crate::timestamp::variant);
        let outcomes = build_outcomes_variant(event);

        let stack = ArgStack::new()
            .set("prediction.id".to_owned(), Variant::String(prediction_id))
            .set("prediction.title".to_owned(), Variant::String(title))
            .set("prediction.outcomes".to_owned(), outcomes);
        let stack = crate::timestamp::set(stack, "prediction.started_at", started_at);
        crate::timestamp::set(stack, "prediction.locks_at", locks_at)
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
                        kind: VariantKind::Datetime,
                        label: "Started at".to_owned(),
                        synthesis: None,
                    },
                    DeclaredVariable {
                        name: "prediction.locks_at".to_owned(),
                        kind: VariantKind::Datetime,
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
    use crate::timestamp::tests::instant;

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
            Some(&instant("2026-06-13T18:00:00Z"))
        );
        assert_eq!(
            stack.get("prediction.locks_at"),
            Some(&instant("2026-06-13T18:02:00Z"))
        );
    }

    fn outcome_row(id: &str, title: &str, color: &str, users: i64, points: i64) -> Variant {
        Variant::Object(std::collections::BTreeMap::from([
            ("id".to_owned(), Variant::String(id.to_owned())),
            ("title".to_owned(), Variant::String(title.to_owned())),
            ("color".to_owned(), Variant::String(color.to_owned())),
            ("users".to_owned(), Variant::Int(users)),
            ("channel_points".to_owned(), Variant::Int(points)),
        ]))
    }

    fn event_with_outcomes(outcomes: serde_json::Value) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.begin",
            serde_json::json!({ "outcomes": outcomes }),
        )
    }

    #[test]
    fn outcomes_keep_wire_order_and_every_field() {
        let event = event_with_outcomes(serde_json::json!([
            {"id": "o-2", "title": "No", "color": "pink", "users": 7, "channel_points": 900},
            {"id": "o-1", "title": "Yes", "color": "blue", "users": 3, "channel_points": 40},
        ]));

        assert_eq!(
            build_outcomes_variant(&event),
            Variant::Array(vec![
                outcome_row("o-2", "No", "pink", 7, 900),
                outcome_row("o-1", "Yes", "blue", 3, 40),
            ])
        );
    }

    #[test]
    fn an_outcome_missing_its_fields_reads_as_blank_with_zero_counts() {
        let event = event_with_outcomes(serde_json::json!([
            {"id": "o-1", "users": null, "channel_points": "many"},
        ]));

        assert_eq!(
            build_outcomes_variant(&event),
            Variant::Array(vec![outcome_row("o-1", "", "", 0, 0)])
        );
    }

    #[test]
    fn an_event_without_an_outcome_list_yields_an_empty_array() {
        for payload in [
            serde_json::json!({}),
            serde_json::json!({ "outcomes": "none" }),
        ] {
            let event = Event::new(
                EventSource::Twitch,
                "twitch.channel.prediction.begin",
                payload,
            );

            assert_eq!(build_outcomes_variant(&event), Variant::Array(Vec::new()));
        }
    }

    #[test]
    fn the_begin_trigger_exposes_the_mapped_outcomes() {
        let event = event_with_outcomes(serde_json::json!([
            {"id": "o-1", "title": "Yes", "color": "blue", "users": 1, "channel_points": 2},
        ]));

        let stack = PredictionStartedDescriptor.build_arg_stack(&event);

        assert_eq!(
            stack.get("prediction.outcomes"),
            Some(&Variant::Array(vec![outcome_row(
                "o-1", "Yes", "blue", 1, 2
            )]))
        );
    }
}
