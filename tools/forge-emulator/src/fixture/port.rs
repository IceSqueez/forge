use std::net::{Ipv4Addr, TcpListener};

use crate::EmulatorError;

/// The port is released before returning, so another process can take it before forge binds.
pub fn free_loopback_port() -> Result<u16, EmulatorError> {
    let probe = |e: std::io::Error| EmulatorError::PortProbe {
        reason: e.to_string(),
    };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(probe)?;
    Ok(listener.local_addr().map_err(probe)?.port())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn probed_port_is_bindable_on_loopback_once_released() {
        let port = free_loopback_port().unwrap();
        assert_ne!(port, 0);
        assert!(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok());
    }
}
