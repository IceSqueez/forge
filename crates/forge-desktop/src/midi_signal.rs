use forge_components::{ForgePalette, tr};
use forge_midi::MidiMonitorEvent;
use forge_types::{TriggerConfig, Variant};
use gpui::Rgba;

pub const MIDI_INPUT_PREFIX: &str = "midi.input.";
pub const NOTE_ON_KIND: &str = "midi.input.note_on";
pub const NOTE_OFF_KIND: &str = "midi.input.note_off";
pub const CONTROL_CHANGE_KIND: &str = "midi.input.control_change";
pub const PITCH_BEND_KIND: &str = "midi.input.pitch_bend";
pub const PROGRAM_CHANGE_KIND: &str = "midi.input.program_change";

pub const SIGNAL_KINDS: [&str; 5] = [
    NOTE_ON_KIND,
    NOTE_OFF_KIND,
    CONTROL_CHANGE_KIND,
    PROGRAM_CHANGE_KIND,
    PITCH_BEND_KIND,
];

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Middle C (MIDI note 60) is named C4.
pub fn note_name(note: i64) -> String {
    let clamped = note.clamp(0, 127);
    let octave = clamped / 12 - 1;
    let name = NOTE_NAMES[(clamped % 12) as usize];
    format!("{name}{octave}")
}

pub fn selector_key(kind_id: &str) -> Option<&'static str> {
    match kind_id {
        NOTE_ON_KIND | NOTE_OFF_KIND => Some("note"),
        CONTROL_CHANGE_KIND => Some("controller"),
        PROGRAM_CHANGE_KIND => Some("program"),
        _ => None,
    }
}

pub fn kind_label(kind_id: &str) -> &str {
    match kind_id {
        NOTE_ON_KIND => "Note On",
        NOTE_OFF_KIND => "Note Off",
        CONTROL_CHANGE_KIND => "CC",
        PROGRAM_CHANGE_KIND => "Program change",
        PITCH_BEND_KIND => "Pitch bend",
        other => other,
    }
}

pub fn kind_color(kind_id: &str, palette: &ForgePalette) -> Rgba {
    match kind_id {
        NOTE_ON_KIND => palette.success,
        NOTE_OFF_KIND => palette.text_faint,
        CONTROL_CHANGE_KIND | PITCH_BEND_KIND => palette.info,
        PROGRAM_CHANGE_KIND => palette.brand,
        _ => palette.text_faint,
    }
}

fn kind_from_monitor(kind: &str) -> Option<&'static str> {
    match kind {
        "note_on" => Some(NOTE_ON_KIND),
        "note_off" => Some(NOTE_OFF_KIND),
        "control_change" => Some(CONTROL_CHANGE_KIND),
        "pitch_bend" => Some(PITCH_BEND_KIND),
        "program_change" => Some(PROGRAM_CHANGE_KIND),
        _ => None,
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
    pub fn from_monitor(event: &MidiMonitorEvent) -> Option<Self> {
        let kind_id = kind_from_monitor(&event.kind)?;
        Some(Self {
            kind_id: kind_id.to_owned(),
            selector: event.number.map(i64::from),
            channel: Some(i64::from(event.channel)),
            device: Some(event.port_name.clone()),
        })
    }

    pub fn from_instance(kind_id: &str, config: &TriggerConfig) -> Self {
        Self {
            kind_id: kind_id.to_owned(),
            selector: selector_key(kind_id).and_then(|key| config_int(config, key)),
            channel: config_int(config, "channel"),
            device: config_text(config, "device"),
        }
    }

    /// Only complete signals can be saved: the selector-bearing kinds need a captured number.
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

    /// Calls `tr!`, which reads a thread-local bundle: render thread only.
    pub fn label(&self) -> String {
        let value = match self.selector {
            Some(n) if self.kind_id == NOTE_ON_KIND || self.kind_id == NOTE_OFF_KIND => {
                note_name(n)
            }
            Some(n) => n.to_string(),
            None => tr!("midi_value_any"),
        };
        match self.kind_id.as_str() {
            NOTE_ON_KIND => format!("Note {value}"),
            NOTE_OFF_KIND => format!("NoteOff {value}"),
            CONTROL_CHANGE_KIND => format!("CC {value}"),
            PROGRAM_CHANGE_KIND => format!("PC {value}"),
            PITCH_BEND_KIND => "Pitch".to_owned(),
            other => other.to_owned(),
        }
    }

    /// Calls `tr!`, which reads a thread-local bundle: render thread only.
    pub fn channel_label(&self) -> String {
        let value = match self.channel {
            Some(channel) => channel.to_string(),
            None => tr!("midi_value_any"),
        };
        format!("ch {value}")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_midi::register_midi_triggers;
    use forge_registry::{FormField, TriggerRegistry};

    use super::*;

    fn monitor_event(kind: &str, number: Option<u8>, channel: u8, port: &str) -> MidiMonitorEvent {
        MidiMonitorEvent {
            kind: kind.to_owned(),
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
                "note_on",
                Some(60),
                signal(NOTE_ON_KIND, Some(60), Some(9), Some("Keys")),
            ),
            (
                "note_off",
                Some(48),
                signal(NOTE_OFF_KIND, Some(48), Some(9), Some("Keys")),
            ),
            (
                "control_change",
                Some(7),
                signal(CONTROL_CHANGE_KIND, Some(7), Some(9), Some("Keys")),
            ),
            (
                "program_change",
                Some(10),
                signal(PROGRAM_CHANGE_KIND, Some(10), Some(9), Some("Keys")),
            ),
            (
                "pitch_bend",
                None,
                signal(PITCH_BEND_KIND, None, Some(9), Some("Keys")),
            ),
        ] {
            let event = monitor_event(monitor_kind, number, 9, "Keys");
            let captured = MidiSignal::from_monitor(&event)
                .unwrap_or_else(|| panic!("{monitor_kind} produced no signal"));
            assert!(
                captured == expected,
                "{monitor_kind} captured the wrong signal"
            );
        }
    }

    #[test]
    fn from_monitor_rejects_a_kind_it_cannot_map_to_a_trigger() {
        let event = monitor_event("aftertouch", Some(60), 0, "Keys");
        assert!(MidiSignal::from_monitor(&event).is_none());
    }

    #[test]
    fn signal_round_trips_through_overrides_for_every_field_combination() {
        for original in [
            signal(NOTE_ON_KIND, Some(60), Some(0), Some("Keys")),
            signal(NOTE_ON_KIND, Some(60), None, None),
            signal(NOTE_OFF_KIND, Some(0), Some(15), None),
            signal(CONTROL_CHANGE_KIND, Some(7), None, Some("Pad")),
            signal(PROGRAM_CHANGE_KIND, Some(127), Some(9), None),
            signal(PITCH_BEND_KIND, None, Some(3), Some("Keys")),
            signal(PITCH_BEND_KIND, None, None, None),
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
        let stray = signal(PITCH_BEND_KIND, Some(5), Some(1), None);
        let written = stray.overrides();

        assert_eq!(written.keys().collect::<Vec<_>>(), vec!["channel"]);
    }

    #[test]
    fn from_instance_reads_an_empty_device_string_as_any_device() {
        let mut config = TriggerConfig::new();
        config.insert("device".to_owned(), Variant::String(String::new()));

        let restored = MidiSignal::from_instance(NOTE_ON_KIND, &config);

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
            (NOTE_ON_KIND, Some(60), "Note C4"),
            (NOTE_OFF_KIND, Some(60), "NoteOff C4"),
            (CONTROL_CHANGE_KIND, Some(7), "CC 7"),
            (PROGRAM_CHANGE_KIND, Some(10), "PC 10"),
            (PITCH_BEND_KIND, None, "Pitch"),
        ] {
            assert_eq!(signal(kind, selector, None, None).label(), expected);
        }
    }

    #[test]
    fn label_replaces_a_missing_selector_with_a_placeholder_instead_of_a_number() {
        let label = signal(CONTROL_CHANGE_KIND, None, None, None).label();

        assert!(label.starts_with("CC "), "{label} lost its kind prefix");
        assert!(
            !label.chars().any(|c| c.is_ascii_digit()),
            "{label} still renders a selector number"
        );
    }

    #[test]
    fn channel_label_renders_the_channel_number_and_a_placeholder_for_any() {
        assert_eq!(
            signal(NOTE_ON_KIND, None, Some(0), None).channel_label(),
            "ch 0"
        );

        let any = signal(NOTE_ON_KIND, None, None, None).channel_label();
        assert!(any.starts_with("ch "), "{any} lost its prefix");
        assert!(
            !any.chars().any(|c| c.is_ascii_digit()),
            "{any} still renders a channel number"
        );
    }
}
