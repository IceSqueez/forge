use super::input::{MidiInputTrigger, SignalValue};
use crate::vocabulary::limits::PITCH_BEND_MAX;
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, trigger_ids};

pub(crate) const PITCH_BEND_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::PITCH_BEND,
    kind: MidiSignalKind::PitchBend,
    label: "MIDI Pitch Bend",
    summary: "Fires when a MIDI Pitch Bend message is received.",
    search_text: "midi pitch bend wheel channel",
    icon: "sliders",
    number: None,
    value: Some(SignalValue {
        variable: variables::VALUE,
        label: "Pitch bend value",
        max: PITCH_BEND_MAX,
    }),
};
