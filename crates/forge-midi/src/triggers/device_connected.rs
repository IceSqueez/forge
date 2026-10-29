use super::port::MidiPortTrigger;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const DEVICE_CONNECTED_TRIGGER: MidiPortTrigger = MidiPortTrigger {
    id: trigger_ids::DEVICE_CONNECTED,
    event_kind: event_kinds::PORT_ADDED,
    label: "MIDI Device Connected",
    summary: "Fires when a MIDI port appears (device plugged in).",
    search_text: "midi device connected port added plugged input output hotplug",
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn port_event(kind: &str, name: &str, direction: &str) -> Event {
        Event::new(
            EventSource::Midi,
            kind,
            json!({ "port_name": name, "direction": direction }),
        )
    }

    fn dir_filter(direction: &str) -> TriggerConfig {
        BTreeMap::from([(
            "direction".to_owned(),
            Variant::String(direction.to_owned()),
        )])
    }

    #[test]
    fn matches_port_added_event() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "input");
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_port_removed_event() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "input");
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_foreign_event_kind() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.input.note_on", "Launchpad", "input");
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn direction_filter_rejects_mismatched_direction() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "output");
        assert!(!d.matches_trigger(&dir_filter("input"), &ev));
    }

    #[test]
    fn direction_filter_accepts_matching_direction() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "input");
        assert!(d.matches_trigger(&dir_filter("input"), &ev));
    }

    #[test]
    fn empty_direction_filter_matches_any_direction() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "output");
        let cfg = BTreeMap::from([("direction".to_owned(), Variant::String(String::new()))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn build_arg_stack_populates_name_and_direction() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "input");
        let stack = d.build_arg_stack(&ev);
        assert_eq!(
            stack.get("midi.device.name"),
            Some(&Variant::String("Launchpad".to_owned())),
        );
        assert_eq!(
            stack.get("midi.device.direction"),
            Some(&Variant::String("input".to_owned())),
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_name() {
        let d = MidiDeviceConnectedDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.port.added",
            json!({ "direction": "input" }),
        );
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.device.name"), None);
        assert_eq!(
            stack.get("midi.device.direction"),
            Some(&Variant::String("input".to_owned())),
        );
    }
}
