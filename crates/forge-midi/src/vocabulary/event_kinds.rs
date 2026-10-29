pub const INPUT_PREFIX: &str = "midi.input.";
pub const PORT_PREFIX: &str = "midi.port.";

pub const NOTE_ON: &str = "midi.input.note_on";
pub const NOTE_OFF: &str = "midi.input.note_off";
pub const CONTROL_CHANGE: &str = "midi.input.control_change";
pub const PITCH_BEND: &str = "midi.input.pitch_bend";
pub const PROGRAM_CHANGE: &str = "midi.input.program_change";

pub const PORT_ADDED: &str = "midi.port.added";
pub const PORT_REMOVED: &str = "midi.port.removed";

pub const ENGINE_ENABLED: &str = "midi.engine.enabled";
pub const ENGINE_DISABLED: &str = "midi.engine.disabled";

pub const SIGNAL_KINDS: [&str; 5] = [
    NOTE_ON,
    NOTE_OFF,
    CONTROL_CHANGE,
    PROGRAM_CHANGE,
    PITCH_BEND,
];
