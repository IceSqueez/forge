#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EndpointSurface {
    env_var: &'static str,
    default_base_url: &'static str,
    protocol: EndpointProtocol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EndpointProtocol {
    Http,
    WebSocket,
}

impl EndpointProtocol {
    pub(crate) fn accepts_scheme(self, scheme: &str) -> bool {
        match self {
            Self::Http => matches!(scheme, "http" | "https"),
            Self::WebSocket => matches!(scheme, "ws" | "wss"),
        }
    }
}

impl EndpointSurface {
    pub const fn http(env_var: &'static str, default_base_url: &'static str) -> Self {
        Self {
            env_var,
            default_base_url,
            protocol: EndpointProtocol::Http,
        }
    }

    pub const fn web_socket(env_var: &'static str, default_base_url: &'static str) -> Self {
        Self {
            env_var,
            default_base_url,
            protocol: EndpointProtocol::WebSocket,
        }
    }

    pub const fn env_var(self) -> &'static str {
        self.env_var
    }

    pub(crate) const fn default_base_url(self) -> &'static str {
        self.default_base_url
    }

    pub(crate) const fn protocol(self) -> EndpointProtocol {
        self.protocol
    }
}
