use time::OffsetDateTime;

use crate::error::ObsError;

pub struct ObsClient {
    #[allow(dead_code)]
    inner: Option<obws::Client>,
    endpoint: String,
    connected_at: Option<OffsetDateTime>,
}

impl ObsClient {
    pub async fn connect(endpoint: &str, password: Option<&str>) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;
        let client = obws::Client::connect(host, port, password)
            .await
            .map_err(map_obws_error)?;
        Ok(Self {
            inner: Some(client),
            endpoint: endpoint.to_owned(),
            connected_at: Some(OffsetDateTime::now_utc()),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn connected_at(&self) -> Option<OffsetDateTime> {
        self.connected_at
    }

    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Result<&obws::Client, ObsError> {
        self.inner.as_ref().ok_or(ObsError::Disconnected)
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        Self {
            inner: None,
            endpoint,
            connected_at: None,
        }
    }
}

fn parse_endpoint(endpoint: &str) -> Result<(String, u16), ObsError> {
    let without_scheme = endpoint
        .strip_prefix("ws://")
        .or_else(|| endpoint.strip_prefix("wss://"))
        .unwrap_or(endpoint);

    match without_scheme.rsplit_once(':') {
        Some((host, port_str)) => {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| ObsError::Connect(format!("invalid port in endpoint '{endpoint}'")))?;
            Ok((host.to_owned(), port))
        }
        None => Ok((without_scheme.to_owned(), 4455)),
    }
}

fn map_obws_error(e: obws::error::Error) -> ObsError {
    match &e {
        obws::error::Error::Timeout => ObsError::Timeout,
        obws::error::Error::Disconnected => ObsError::Disconnected,
        obws::error::Error::Handshake(obws::client::HandshakeError::NoIdentified) => {
            ObsError::Authentication
        }
        _ => ObsError::Connect(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_roundtrip() {
        let client = ObsClient::new_for_test("ws://localhost:4455".to_owned());
        assert_eq!(client.endpoint(), "ws://localhost:4455");
    }

    #[test]
    fn connected_at_none_for_test_constructor() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        assert!(client.connected_at().is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_with_scheme_and_port() {
        let (host, port) = parse_endpoint("ws://localhost:4455").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 4455);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_without_scheme() {
        let (host, port) = parse_endpoint("192.168.1.10:4455").unwrap();
        assert_eq!(host, "192.168.1.10");
        assert_eq!(port, 4455);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn parse_endpoint_default_port() {
        let (host, port) = parse_endpoint("localhost").unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 4455);
    }

    #[test]
    fn parse_endpoint_invalid_port_errors() {
        assert!(parse_endpoint("localhost:notaport").is_err());
    }
}
