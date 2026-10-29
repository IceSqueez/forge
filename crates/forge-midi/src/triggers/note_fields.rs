use super::input::{SignalNumber, SignalValue};
use crate::vocabulary::config_keys;
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::variables::input as variables;

pub(super) const NOTE: SignalNumber = SignalNumber {
    config_key: config_keys::NOTE,
    optional_label: "Note (leave empty for any)",
    label: "Note",
    display_name: "note",
    any_display: "any note",
    variable: variables::NOTE,
    variable_label: "Note number",
};

pub(super) const VELOCITY: SignalValue = SignalValue {
    variable: variables::VELOCITY,
    label: "Velocity",
    max: DATA_BYTE_MAX,
};
