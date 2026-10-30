use super::input::{SignalNumber, SignalValue};
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{config_keys, messages};

pub(super) const NOTE: SignalNumber = SignalNumber {
    config_key: config_keys::NOTE,
    optional_label: "Note (leave empty for any)",
    label: "Note",
    filter_condition: messages::CONDITION_NOTE,
    any_condition: messages::CONDITION_ANY_NOTE,
    variable: variables::NOTE,
    variable_label: "Note number",
};

pub(super) const VELOCITY: SignalValue = SignalValue {
    variable: variables::VELOCITY,
    label: "Velocity",
    max: DATA_BYTE_MAX,
};
