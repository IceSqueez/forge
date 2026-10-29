use super::event_kinds;

pub const NOTE_ON: &str = event_kinds::NOTE_ON;
pub const NOTE_OFF: &str = event_kinds::NOTE_OFF;
pub const CONTROL_CHANGE: &str = event_kinds::CONTROL_CHANGE;
pub const PITCH_BEND: &str = event_kinds::PITCH_BEND;
pub const PROGRAM_CHANGE: &str = event_kinds::PROGRAM_CHANGE;

pub const DEVICE_CONNECTED: &str = "midi.device.connected";
pub const DEVICE_DISCONNECTED: &str = "midi.device.disconnected";
