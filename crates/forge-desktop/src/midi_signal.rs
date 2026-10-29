use forge_components::{ForgePalette, tr};
use forge_midi::vocabulary::event_kinds::{
    CONTROL_CHANGE, NOTE_OFF, NOTE_ON, PITCH_BEND, PROGRAM_CHANGE,
};
use forge_midi::{MidiMonitorEvent, MidiSignalKind};
use forge_types::{TriggerConfig, Variant};
use gpui::Rgba;

const MIDI_NOTE_MAX: i64 = 127;
const SEMITONES_PER_OCTAVE: i64 = 12;
const NOTE_NAMES: [&str; SEMITONES_PER_OCTAVE as usize] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

pub fn note_name(note: i64) -> String {
    let clamped = note.clamp(0, MIDI_NOTE_MAX);
    let octave = clamped / SEMITONES_PER_OCTAVE - 1;
    let name = NOTE_NAMES[(clamped % SEMITONES_PER_OCTAVE) as usize];
    format!("{name}{octave}")
}

pub fn selector_key(kind_id: &str) -> Option<&'static str> {
    match kind_id {
        NOTE_ON | NOTE_OFF => Some("note"),
        CONTROL_CHANGE => Some("controller"),
        PROGRAM_CHANGE => Some("program"),
        _ => None,
    }
}

pub fn kind_label(kind_id: &str) -> &str {
    match kind_id {
        NOTE_ON => "Note On",
        NOTE_OFF => "Note Off",
        CONTROL_CHANGE => "CC",
        PROGRAM_CHANGE => "Program change",
        PITCH_BEND => "Pitch bend",
        other => other,
    }
}

pub fn short_kind_label(kind: MidiSignalKind) -> &'static str {
    match kind {
        MidiSignalKind::NoteOn => "Note On",
        MidiSignalKind::NoteOff => "Note Off",
        MidiSignalKind::ControlChange => "CC",
        MidiSignalKind::PitchBend => "Pitch",
        MidiSignalKind::ProgramChange => "PC",
    }
}

pub fn kind_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    match kind_id {
        NOTE_ON => palette.success,
        NOTE_OFF => palette.text_faint,
        CONTROL_CHANGE | PITCH_BEND => palette.info,
        PROGRAM_CHANGE => palette.theme_brand,
        _ => palette.text_faint,
    }
}

fn config_int(config: &TriggerConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(n)) => Some(*n),
        _ => None,
    }
}

pub fn config_text(config: &TriggerConfig, key: &str) -> Option<String> {
    match config.get(key) {
        Some(Variant::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct MidiSignal {
    pub kind_id: String,
    pub selector: Option<i64>,
    pub channel: Option<i64>,
    pub device: Option<String>,
}

impl MidiSignal {
    pub fn from_monitor(event: &MidiMonitorEvent) -> Self {
        Self {
            kind_id: event.kind.event_kind().to_owned(),
            selector: event.number.map(i64::from),
            channel: Some(i64::from(event.channel)),
            device: Some(event.port_name.clone()),
        }
    }

    pub fn from_instance(kind_id: &str, config: &TriggerConfig) -> Self {
        Self {
            kind_id: kind_id.to_owned(),
            selector: selector_key(kind_id).and_then(|key| config_int(config, key)),
            channel: config_int(config, "channel"),
            device: config_text(config, "device"),
        }
    }

    pub fn is_complete(&self) -> bool {
        selector_key(&self.kind_id).is_none() || self.selector.is_some()
    }

    pub fn overrides(&self) -> TriggerConfig {
        let mut config = TriggerConfig::new();
        if let (Some(key), Some(value)) = (selector_key(&self.kind_id), self.selector) {
            config.insert(key.to_owned(), Variant::Int(value));
        }
        if let Some(channel) = self.channel {
            config.insert("channel".to_owned(), Variant::Int(channel));
        }
        if let Some(device) = &self.device {
            config.insert("device".to_owned(), Variant::String(device.clone()));
        }
        config
    }

    pub fn label(&self) -> String {
        let value = match self.selector {
            Some(n) if self.kind_id == NOTE_ON || self.kind_id == NOTE_OFF => note_name(n),
            Some(n) => n.to_string(),
            None => tr!("midi_value_any"),
        };
        match self.kind_id.as_str() {
            NOTE_ON => format!("Note {value}"),
            NOTE_OFF => format!("NoteOff {value}"),
            CONTROL_CHANGE => format!("CC {value}"),
            PROGRAM_CHANGE => format!("PC {value}"),
            PITCH_BEND => "Pitch".to_owned(),
            other => other.to_owned(),
        }
    }

    pub fn channel_label(&self) -> String {
        let value = match self.channel {
            Some(channel) => channel.to_string(),
            None => tr!("midi_value_any"),
        };
        format!("ch {value}")
    }
}

#[cfg(test)]
mod tests {
    use forge_midi::register_midi_triggers;
    use forge_registry::{FormField, TriggerRegistry};

    use super::*;
    use forge_midi::MidiSignalKind;
    use forge_midi::vocabulary::event_kinds::SIGNAL_KINDS;

    fn monitor_event(
        kind: MidiSignalKind,
        number: Option<u8>,
        channel: u8,
        port: &str,
    ) -> MidiMonitorEvent {
        MidiMonitorEvent {
            kind,
            port_name: port.to_owned(),
            channel,
            number,
            value: None,
        }
    }

    fn signal(
        kind: &str,
        selector: Option<i64>,
        channel: Option<i64>,
        device: Option<&str>,
    ) -> MidiSignal {
        MidiSignal {
            kind_id: kind.to_owned(),
            selector,
            channel,
            device: device.map(str::to_owned),
        }
    }

    fn optional_field_keys(fields: &[FormField]) -> Vec<&'static str> {
        fields
            .iter()
            .filter_map(|f| match f {
                FormField::Optional { key, .. } => Some(*key),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn note_name_maps_midi_numbers_to_scientific_pitch() {
        for (note, expected) in [
            (0, "C-1"),
            (11, "B-1"),
            (12, "C0"),
            (59, "B3"),
            (60, "C4"),
            (61, "C#4"),
            (127, "G9"),
        ] {
            assert_eq!(note_name(note), expected, "note {note}");
        }
    }

    #[test]
    fn note_name_clamps_numbers_outside_the_midi_range() {
        for (note, expected) in [
            (-1, "C-1"),
            (i64::MIN, "C-1"),
            (128, "G9"),
            (i64::MAX, "G9"),
        ] {
            assert_eq!(note_name(note), expected, "note {note}");
        }
    }

    #[test]
    fn every_signal_kind_has_a_label_that_is_not_its_raw_id() {
        for kind in SIGNAL_KINDS {
            assert_ne!(
                kind_label(kind),
                kind,
                "{kind} falls through to the raw-id branch"
            );
        }
    }

    #[test]
    fn every_signal_kind_writes_config_keys_its_trigger_descriptor_declares() {
        let mut registry = TriggerRegistry::new();
        register_midi_triggers(&mut registry).unwrap();

        for kind in SIGNAL_KINDS {
            let descriptor = registry
                .get(kind)
                .unwrap_or_else(|| panic!("{kind} is not a registered MIDI trigger"));
            let keys = optional_field_keys(&descriptor.config_fields());
            for key in selector_key(kind).into_iter().chain(["channel", "device"]) {
                assert!(keys.contains(&key), "{kind} declares no {key} field");
            }
        }
    }

    #[test]
    fn from_monitor_maps_each_supported_kind_and_its_fields() {
        for (monitor_kind, number, expected) in [
            (
                MidiSignalKind::NoteOn,
                Some(60),
                signal(NOTE_ON, Some(60), Some(9), Some("Keys")),
            ),
            (
                MidiSignalKind::NoteOff,
                Some(48),
                signal(NOTE_OFF, Some(48), Some(9), Some("Keys")),
            ),
            (
                MidiSignalKind::ControlChange,
                Some(7),
                signal(CONTROL_CHANGE, Some(7), Some(9), Some("Keys")),
            ),
            (
                MidiSignalKind::ProgramChange,
                Some(10),
                signal(PROGRAM_CHANGE, Some(10), Some(9), Some("Keys")),
            ),
            (
                MidiSignalKind::PitchBend,
                None,
                signal(PITCH_BEND, None, Some(9), Some("Keys")),
            ),
        ] {
            let event = monitor_event(monitor_kind, number, 9, "Keys");
            let captured = MidiSignal::from_monitor(&event);
            assert!(
                captured == expected,
                "{monitor_kind:?} captured the wrong signal"
            );
        }
    }

    #[test]
    fn signal_round_trips_through_overrides_for_every_field_combination() {
        for original in [
            signal(NOTE_ON, Some(60), Some(0), Some("Keys")),
            signal(NOTE_ON, Some(60), None, None),
            signal(NOTE_OFF, Some(0), Some(15), None),
            signal(CONTROL_CHANGE, Some(7), None, Some("Pad")),
            signal(PROGRAM_CHANGE, Some(127), Some(9), None),
            signal(PITCH_BEND, None, Some(3), Some("Keys")),
            signal(PITCH_BEND, None, None, None),
        ] {
            let restored = MidiSignal::from_instance(&original.kind_id, &original.overrides());
            assert!(
                restored == original,
                "{} lost a field through overrides",
                original.kind_id
            );
        }
    }

    #[test]
    fn overrides_drops_a_selector_the_kind_cannot_carry() {
        let stray = signal(PITCH_BEND, Some(5), Some(1), None);
        let written = stray.overrides();

        assert_eq!(written.keys().collect::<Vec<_>>(), vec!["channel"]);
    }

    #[test]
    fn from_instance_reads_an_empty_device_string_as_any_device() {
        let mut config = TriggerConfig::new();
        config.insert("device".to_owned(), Variant::String(String::new()));

        let restored = MidiSignal::from_instance(NOTE_ON, &config);

        assert_eq!(restored.device, None);
    }

    #[test]
    fn is_complete_requires_a_selector_only_for_selector_bearing_kinds() {
        for kind in SIGNAL_KINDS {
            let needs_selector = selector_key(kind).is_some();
            assert_eq!(
                signal(kind, None, None, None).is_complete(),
                !needs_selector,
                "{kind} without a selector"
            );
            assert!(
                signal(kind, Some(1), None, None).is_complete(),
                "{kind} with a selector"
            );
        }
    }

    #[test]
    fn label_renders_note_kinds_as_pitch_names_and_the_rest_as_raw_numbers() {
        for (kind, selector, expected) in [
            (NOTE_ON, Some(60), "Note C4"),
            (NOTE_OFF, Some(60), "NoteOff C4"),
            (CONTROL_CHANGE, Some(7), "CC 7"),
            (PROGRAM_CHANGE, Some(10), "PC 10"),
            (PITCH_BEND, None, "Pitch"),
        ] {
            assert_eq!(signal(kind, selector, None, None).label(), expected);
        }
    }

    #[test]
    fn label_replaces_a_missing_selector_with_a_placeholder_instead_of_a_number() {
        let label = signal(CONTROL_CHANGE, None, None, None).label();

        assert!(label.starts_with("CC "), "{label} lost its kind prefix");
        assert!(
            !label.chars().any(|c| c.is_ascii_digit()),
            "{label} still renders a selector number"
        );
    }

    #[test]
    fn channel_label_renders_the_channel_number_and_a_placeholder_for_any() {
        assert_eq!(signal(NOTE_ON, None, Some(0), None).channel_label(), "ch 0");

        let any = signal(NOTE_ON, None, None, None).channel_label();
        assert!(any.starts_with("ch "), "{any} lost its prefix");
        assert!(
            !any.chars().any(|c| c.is_ascii_digit()),
            "{any} still renders a channel number"
        );
    }
}
