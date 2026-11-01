use forge_registry::{EventVariable, TriggerVariables, event_variables};

use crate::vocabulary::fields::{item, model, tracking};
use crate::vocabulary::variables;

const TRACKING: &[EventVariable] = &[
    EventVariable::flag(
        variables::tracking::IS_LEFT_HAND_FOUND,
        "Left hand tracked",
        tracking::IS_LEFT_HAND_FOUND,
    ),
    EventVariable::flag(
        variables::tracking::IS_RIGHT_HAND_FOUND,
        "Right hand tracked",
        tracking::IS_RIGHT_HAND_FOUND,
    ),
];

pub(super) fn tracking_variables() -> Option<TriggerVariables> {
    event_variables(TRACKING)
}

const MODEL: &[EventVariable] = &[
    EventVariable::text(variables::model::NAME, "Model name", model::MODEL_NAME),
    EventVariable::text(variables::model::ID, "Model ID", model::MODEL_ID),
];

pub(super) fn model_variables() -> Option<TriggerVariables> {
    event_variables(MODEL)
}

const ITEM: &[EventVariable] = &[
    EventVariable::text(
        variables::item::INSTANCE_ID,
        "Item instance ID",
        item::ITEM_INSTANCE_ID,
    ),
    EventVariable::text(
        variables::item::FILE_NAME,
        "Item file name",
        item::ITEM_FILE_NAME,
    ),
];

pub(super) fn item_variables() -> Option<TriggerVariables> {
    event_variables(ITEM)
}
