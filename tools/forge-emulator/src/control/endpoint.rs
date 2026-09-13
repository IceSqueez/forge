use std::net::{IpAddr, SocketAddr};

use crate::EmulatorError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlEndpoint {
    url: String,
}

impl ControlEndpoint {
    /// Refuses every non-loopback host: the bearer secret travels over plain `ws://`.
    pub fn loopback(host: &str, port: u16) -> Result<Self, EmulatorError> {
        if port == 0 {
            return Err(EmulatorError::ZeroPort);
        }
        let authority = if host.eq_ignore_ascii_case("localhost") {
            format!("localhost:{port}")
        } else {
            let bare = host
                .strip_prefix('[')
                .and_then(|inner| inner.strip_suffix(']'))
                .unwrap_or(host);
            match bare.parse::<IpAddr>() {
                Ok(ip) if ip.is_loopback() => SocketAddr::new(ip, port).to_string(),
                _ => {
                    return Err(EmulatorError::NonLoopbackHost {
                        host: host.to_owned(),
                    });
                }
            }
        };
        Ok(Self {
            url: format!("ws://{authority}/ws/v1/"),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_build_the_versioned_control_url() {
        for (host, port, expected) in [
            ("127.0.0.1", 8081, "ws://127.0.0.1:8081/ws/v1/"),
            ("127.10.0.1", 1, "ws://127.10.0.1:1/ws/v1/"),
            ("localhost", 65535, "ws://localhost:65535/ws/v1/"),
            ("LocalHost", 8081, "ws://localhost:8081/ws/v1/"),
            ("::1", 8081, "ws://[::1]:8081/ws/v1/"),
            ("[::1]", 8081, "ws://[::1]:8081/ws/v1/"),
        ] {
            let endpoint = ControlEndpoint::loopback(host, port)
                .unwrap_or_else(|e| panic!("{host}:{port} refused: {e}"));
            assert_eq!(endpoint.url(), expected, "host {host:?}");
        }
    }

    #[test]
    fn non_loopback_hosts_are_refused_before_any_connection() {
        for host in [
            "192.0.2.1",
            "0.0.0.0",
            "::",
            "::ffff:127.0.0.1",
            "example.com",
            "localhost.example.com",
            "127.0.0.1.example.com",
            "",
        ] {
            let refusal = ControlEndpoint::loopback(host, 8081);
            assert!(
                matches!(&refusal, Err(EmulatorError::NonLoopbackHost { host: echoed }) if echoed == host),
                "expected refusal for {host:?}, got {refusal:?}"
            );
        }
    }

    #[test]
    fn port_zero_is_refused() {
        assert!(matches!(
            ControlEndpoint::loopback("127.0.0.1", 0),
            Err(EmulatorError::ZeroPort)
        ));
    }
}
