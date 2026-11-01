use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::prediction as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text(
        "prediction.id",
        "Prediction ID",
        fields::PREDICTION,
        fields::PREDICTION_ID,
    ),
    Variable::nested_text(
        "prediction.title",
        "Prediction title",
        fields::PREDICTION,
        fields::PREDICTION_TITLE,
    ),
    Variable::nested_text(
        "prediction.winning_outcome_id",
        "Winning outcome ID",
        fields::PREDICTION,
        fields::WINNING_OUTCOME_ID,
    ),
    Variable::nested_text(
        "prediction.status",
        "Prediction status",
        fields::PREDICTION,
        fields::STATUS,
    ),
    Variable::nested_timestamp(
        "prediction.ended_at",
        "Ended at",
        fields::PREDICTION,
        fields::ENDED_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::PREDICTION_ENDED,
    category: TriggerCategory::Predictions,
    label: "Prediction ended",
    summary: "Fires when a prediction resolves or is canceled",
    search_text: "twitch prediction ended resolved canceled winner outcome",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_PREDICTION_END,
    actors: ActorDeclaration::Actorless,
    variables,
    ..TwitchTriggerSpec::BASE
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timestamp::tests::instant;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;

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
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.end")
        );
    }

    #[test]
    fn build_arg_stack_carries_winning_outcome_and_status_fields() {
        let stack = SPEC.build_arg_stack(&prediction_end_event());
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
            Some(&instant("2026-06-13T18:10:00Z"))
        );
    }
}
