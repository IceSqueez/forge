use super::spec::{TwitchTriggerSpec, Variable, table_variables};
use crate::vocabulary::event_kinds;
use crate::vocabulary::trigger_ids;
use forge_registry::TriggerVariables;
use forge_registry::{ActorDeclaration, TriggerCategory};

use crate::vocabulary::fields::prediction as fields;

const VARIABLES: &[Variable] = &[
    Variable::nested_text("prediction.id", fields::PREDICTION, fields::PREDICTION_ID),
    Variable::nested_text(
        "prediction.title",
        fields::PREDICTION,
        fields::PREDICTION_TITLE,
    ),
    Variable::nested_timestamp(
        "prediction.locked_at",
        fields::PREDICTION,
        fields::LOCKED_AT,
    ),
];

fn variables() -> Option<TriggerVariables> {
    Some(table_variables(VARIABLES))
}

pub(super) const SPEC: TwitchTriggerSpec = TwitchTriggerSpec {
    id: trigger_ids::PREDICTION_LOCKED,
    category: TriggerCategory::Predictions,
    label: "Prediction locked",
    summary: "Fires when a prediction is locked and no more points can be placed",
    search_text: "twitch prediction locked closed outcome",
    icon: "chart-bar",
    kind: event_kinds::CHANNEL_PREDICTION_LOCK,
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

    fn prediction_lock_event() -> Event {
        let payload = serde_json::json!({
            "prediction": {
                "id": "pred-3",
                "title": "Final score?",
                "locked_at": "2026-06-13T18:05:00Z",
            },
        });
        Event::new(
            EventSource::Twitch,
            "twitch.channel.prediction.lock",
            payload,
        )
    }

    #[test]
    fn event_filter_targets_prediction_lock_topic_from_twitch() {
        let filter = SPEC.event_filter();
        assert_eq!(filter.source, Some(EventSource::Twitch));
        assert_eq!(
            filter.kind_prefix.as_deref(),
            Some("twitch.channel.prediction.lock")
        );
    }

    #[test]
    fn build_arg_stack_maps_prediction_id_title_and_locked_at() {
        let stack = SPEC.build_arg_stack(&prediction_lock_event());
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
            Some(&instant("2026-06-13T18:05:00Z"))
        );
    }
}
