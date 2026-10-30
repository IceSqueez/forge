use super::input::{MidiInputTrigger, SignalNumber};
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, messages, trigger_ids};

pub(crate) const PROGRAM_CHANGE_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::PROGRAM_CHANGE,
    kind: MidiSignalKind::ProgramChange,
    label: "MIDI Program Change",
    summary: "Fires when a MIDI Program Change message is received.",
    search_text: "midi program change patch preset channel",
    icon: "sliders",
    number: Some(SignalNumber {
        config_key: config_keys::PROGRAM,
        optional_label: "Program (leave empty for any)",
        label: "Program",
        filter_condition: messages::CONDITION_PROGRAM,
        any_condition: messages::CONDITION_ANY_PROGRAM,
        variable: variables::PROGRAM,
        variable_label: "Program number",
    }),
    value: None,
};
