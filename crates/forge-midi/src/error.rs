use thiserror::Error;

#[derive(Debug, Error)]
pub enum MidiError {
    #[error("MIDI port not found: {name:?}")]
    PortNotFound { name: String },

    #[error("invalid status byte: 0x{0:02X}")]
    InvalidStatusByte(u8),

    #[error("MIDI value {value} is above its maximum of {max}")]
    ValueOutOfRange { value: u8, max: i64 },

    #[error("midir initialization failed: {0}")]
    MidirInit(String),

    #[error("midir connection failed: {0}")]
    MidirConnect(String),

    #[error("output send failed: {0}")]
    OutputSend(String),

    #[error("MIDI supervisor task is not running")]
    SupervisorUnavailable,
}
