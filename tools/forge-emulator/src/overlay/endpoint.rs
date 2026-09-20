use std::net::{Ipv4Addr, SocketAddr};

use crate::EmulatorError;

const CONTROL_PATH: &str = "/ws/v1/";
const OVERLAY_ROOT: &str = "/overlays";
const CONFIG_DOCUMENT: &str = "config.json";

/// The loopback authority a browser source would have been served from: the page's own origin,
/// its `config.json` URL and the control socket it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEndpoint {
    authority: String,
}

impl PageEndpoint {
    pub fn loopback(port: u16) -> Result<Self, EmulatorError> {
        if port == 0 {
            return Err(EmulatorError::ZeroPort);
        }
        Ok(Self {
            authority: SocketAddr::from((Ipv4Addr::LOCALHOST, port)).to_string(),
        })
    }

    pub fn origin(&self) -> String {
        format!("http://{}", self.authority)
    }

    pub fn config_url(&self, identity: &str) -> String {
        format!(
            "http://{}{OVERLAY_ROOT}/{identity}/{CONFIG_DOCUMENT}",
            self.authority
        )
    }

    pub fn socket_url(&self) -> String {
        format!("ws://{}{CONTROL_PATH}", self.authority)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_addressed_exactly_as_the_browser_source_that_forge_generated_would_be() {
        let endpoint = PageEndpoint::loopback(41234).expect("a non-zero port");

        assert_eq!(endpoint.origin(), "http://127.0.0.1:41234");
        assert_eq!(
            endpoint.config_url("alert-box"),
            "http://127.0.0.1:41234/overlays/alert-box/config.json"
        );
        assert_eq!(endpoint.socket_url(), "ws://127.0.0.1:41234/ws/v1/");
    }

    #[test]
    fn port_zero_is_refused() {
        assert!(matches!(
            PageEndpoint::loopback(0),
            Err(EmulatorError::ZeroPort)
        ));
    }
}
