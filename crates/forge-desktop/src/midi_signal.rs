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
