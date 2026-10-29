use super::port::MidiPortTrigger;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const DEVICE_CONNECTED_TRIGGER: MidiPortTrigger = MidiPortTrigger {
    id: trigger_ids::DEVICE_CONNECTED,
    event_kind: event_kinds::PORT_ADDED,
    label: "MIDI Device Connected",
    summary: "Fires when a MIDI port appears (device plugged in).",
    search_text: "midi device connected port added plugged input output hotplug",
};
