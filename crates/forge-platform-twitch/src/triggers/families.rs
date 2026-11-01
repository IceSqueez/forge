use forge_events::Event;
use forge_registry::TriggerVariables;
use forge_types::Variant;

use super::spec::{Variable, table_variables};
use crate::vocabulary::fields::charity as charity_fields;
use crate::vocabulary::fields::poll as fields;
use crate::vocabulary::fields::prediction as prediction_fields;
use crate::vocabulary::fields::reward as reward_fields;

const CHARITY: &[Variable] = &[
    Variable::nested_text(
        "charity.id",
        charity_fields::CHARITY,
        charity_fields::CHARITY_ID,
    ),
    Variable::nested_text(
        "charity.name",
        charity_fields::CHARITY,
        charity_fields::CHARITY_NAME,
    ),
    Variable::nested_int(
        "charity.current_amount_cents",
        charity_fields::CHARITY,
        charity_fields::CURRENT_AMOUNT_CENTS,
    )
    .bounded(0, super::MAX_SYNTHESIZED_COUNT),
    Variable::nested_int(
        "charity.target_amount_cents",
        charity_fields::CHARITY,
        charity_fields::TARGET_AMOUNT_CENTS,
    )
    .bounded(0, super::MAX_SYNTHESIZED_COUNT),
    Variable::nested_text(
        "charity.currency_code",
        charity_fields::CHARITY,
        charity_fields::CURRENCY_CODE,
    ),
    Variable::nested_int(
        "charity.decimal_places",
        charity_fields::CHARITY,
        charity_fields::DECIMAL_PLACES,
    )
    .bounded(0, 3),
];

pub(super) fn charity_variables() -> Option<TriggerVariables> {
    Some(table_variables(CHARITY))
}

pub(super) fn build_choices_variant(event: &Event) -> Variant {
    let choices = event
        .payload
        .get(fields::CHOICES)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    Variant::Array(
        choices
            .iter()
            .map(|choice| {
                let mut obj = std::collections::BTreeMap::new();
                obj.insert(
                    "id".to_owned(),
                    Variant::String(
                        choice
                            .get(fields::CHOICE_ID)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "title".to_owned(),
                    Variant::String(
                        choice
                            .get(fields::CHOICE_TITLE)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "votes".to_owned(),
                    Variant::Int(
                        choice
                            .get(fields::CHOICE_VOTES)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                Variant::Object(obj)
            })
            .collect(),
    )
}

pub(super) fn build_outcomes_variant(event: &Event) -> Variant {
    let outcomes = event
        .payload
        .get(prediction_fields::OUTCOMES)
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
                            .get(prediction_fields::OUTCOME_ID)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "title".to_owned(),
                    Variant::String(
                        outcome
                            .get(prediction_fields::OUTCOME_TITLE)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "color".to_owned(),
                    Variant::String(
                        outcome
                            .get(prediction_fields::OUTCOME_COLOR)
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                    ),
                );
                obj.insert(
                    "users".to_owned(),
                    Variant::Int(
                        outcome
                            .get(prediction_fields::OUTCOME_USERS)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                obj.insert(
                    "channel_points".to_owned(),
                    Variant::Int(
                        outcome
                            .get(prediction_fields::OUTCOME_CHANNEL_POINTS)
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                    ),
                );
                Variant::Object(obj)
            })
            .collect(),
    )
}

const REWARD: &[Variable] = &[
    Variable::nested_text("reward.id", reward_fields::REWARD, reward_fields::REWARD_ID),
    Variable::nested_text(
        "reward.title",
        reward_fields::REWARD,
        reward_fields::REWARD_TITLE,
    ),
    Variable::nested_int(
        "reward.cost",
        reward_fields::REWARD,
        reward_fields::REWARD_COST,
    )
    .bounded(0, super::MAX_SYNTHESIZED_COUNT),
    Variable::nested_text(
        "reward.prompt",
        reward_fields::REWARD,
        reward_fields::REWARD_PROMPT,
    ),
    Variable::nested_flag(
        "reward.is_enabled",
        reward_fields::REWARD,
        reward_fields::REWARD_IS_ENABLED,
    ),
];

pub(super) fn reward_variables() -> Option<TriggerVariables> {
    Some(table_variables(REWARD))
}
