use super::input::{MidiInputTrigger, SignalNumber, SignalValue};
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, messages, trigger_ids};

pub(crate) const CONTROL_CHANGE_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::CONTROL_CHANGE,
    kind: MidiSignalKind::ControlChange,
    label: "MIDI Control Change",
    summary: "Fires when a MIDI Control Change (CC) message is received.",
    search_text: "midi cc control change controller knob fader channel",
    icon: "sliders",
    number: Some(SignalNumber {
        config_key: config_keys::CONTROLLER,
        optional_label: "Controller (leave empty for any)",
        label: "Controller",
        filter_condition: messages::CONDITION_CONTROLLER,
        any_condition: messages::CONDITION_ANY_CONTROLLER,
        variable: variables::CONTROLLER,
        variable_label: "Controller number",
    }),
    value: Some(SignalValue {
        variable: variables::VALUE,
        label: "Controller value",
        max: DATA_BYTE_MAX,
    }),
};
