use thiserror::Error;

#[derive(Debug, Error)]
pub enum MidiError {
    #[error("MIDI port not found: {name:?}")]
    PortNotFound { name: String },

    #[error("MIDI port disconnected: {name:?}")]
    PortDisconnected { name: String },

    #[error("invalid status byte: 0x{0:02X}")]
    InvalidStatusByte(u8),

    #[error("midir initialization failed: {0}")]
    MidirInit(String),

    #[error("midir connection failed: {0}")]
    MidirConnect(String),

    #[error("output send failed: {0}")]
    OutputSend(String),

    #[error("MIDI supervisor task is not running")]
    SupervisorUnavailable,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn port_not_found_display_contains_name() {
        let e = MidiError::PortNotFound {
            name: "Piano".to_owned(),
        };
        assert!(e.to_string().contains("Piano"));
    }

    #[test]
    fn invalid_status_byte_display_is_hex() {
        let e = MidiError::InvalidStatusByte(0x7F);
        assert!(e.to_string().contains("7F"));
    }

    #[test]
    fn midir_init_display_contains_message() {
        let e = MidiError::MidirInit("init failed".to_owned());
        assert!(e.to_string().contains("init failed"));
    }
}
