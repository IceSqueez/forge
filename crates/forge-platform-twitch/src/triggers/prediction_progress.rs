use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};
use forge_types::VariantKind;

use super::families::build_outcomes_variant;
use crate::vocabulary::fields::prediction as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("prediction.id", fields::PREDICTION, fields::PREDICTION_ID),
    Variable::nested_text(
        "prediction.title",
        fields::PREDICTION,
        fields::PREDICTION_TITLE,
    ),
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
    id: trigger_ids::PREDICTION_PROGRESS,
    category: TriggerCategory::Predictions,
    label: "Prediction progress",
    summary: "Fires when point totals update on an active prediction",
    search_text: "twitch prediction progress update outcome points",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_PREDICTION_PROGRESS,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

    fn prediction_progress_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-2",
                "title": "Next round outcome",
            },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.progress",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_prediction_progress_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.progress")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_and_title() {
        let stack = SPEC.build_arg_stack(&prediction_progress_event());
        assert_eq!(
            stack.get("prediction.id"),
            Some(&Variant::String("pred-2".to_owned()))
        );
        assert_eq!(
            stack.get("prediction.title"),
            Some(&Variant::String("Next round outcome".to_owned()))
        );
    }

    #[test]
    fn the_progress_trigger_exposes_the_running_outcome_counts() {
        let event = Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.progress",
            serde_json::json!({
                "outcomes": [{"id": "o-1", "users": 11, "channel_points": 250}],
            }),
        );

        let stack = SPEC.build_arg_stack(&event);
        let Some(Variant::Array(outcomes)) = stack.get("prediction.outcomes") else {
            panic!("outcomes missing from the progress stack");
        };

        assert_eq!(
            outcomes.as_slice(),
            [Variant::Object(std::collections::BTreeMap::from([
                ("id".to_owned(), Variant::String("o-1".to_owned())),
                ("title".to_owned(), Variant::String(String::new())),
                ("color".to_owned(), Variant::String(String::new())),
                ("users".to_owned(), Variant::Int(11)),
                ("channel_points".to_owned(), Variant::Int(250)),
            ]))]
        );
    }
}
