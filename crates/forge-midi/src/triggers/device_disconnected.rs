use super::port::MidiPortTrigger;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const DEVICE_DISCONNECTED_TRIGGER: MidiPortTrigger = MidiPortTrigger {
    id: trigger_ids::DEVICE_DISCONNECTED,
    event_kind: event_kinds::PORT_REMOVED,
    label: "MIDI Device Disconnected",
    summary: "Fires when a MIDI port disappears (device unplugged).",
    search_text: "midi device disconnected port removed unplugged input output hotplug",
};
