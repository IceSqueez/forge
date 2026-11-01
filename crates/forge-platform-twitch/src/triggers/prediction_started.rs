use super::families::build_outcomes_variant;
use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::VariantKind;

use crate::vocabulary::fields::prediction as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("prediction.id", fields::PREDICTION, fields::PREDICTION_ID),
    Variable::nested_text(
        "prediction.title",
        fields::PREDICTION,
        fields::PREDICTION_TITLE,
    ),
    Variable::nested_timestamp(
        "prediction.started_at",
        fields::PREDICTION,
        fields::STARTED_AT,
    ),
    Variable::nested_timestamp("prediction.locks_at", fields::PREDICTION, fields::LOCKS_AT),
    Variable::custom(
        "prediction.outcomes",
        VariantKind::Array,
        build_outcomes_variant,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::PREDICTION_STARTED,
    category: TriggerCategory::Predictions,
    label: "Prediction started",
    summary: "Fires when a prediction begins on the broadcaster's channel",
    search_text: "twitch prediction started begin outcome",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_PREDICTION_BEGIN,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::Event;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

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
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.begin")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_title_and_timing_fields() {
        let stack = SPEC.build_arg_stack(&prediction_begin_event());
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

        let stack = SPEC.build_arg_stack(&event);

        assert_eq!(
            stack.get("prediction.outcomes"),
            Some(&Variant::Array(vec![outcome_row(
                "o-1", "Yes", "blue", 1, 2
            )]))
        );
    }
}
