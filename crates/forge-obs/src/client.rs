use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use rand::RngExt;
use time::OffsetDateTime;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use forge_platform_core::ConnectionState;

use crate::error::ObsError;

const STATE_DISCONNECTED: u8 = 0;
const STATE_CONNECTING: u8 = 1;
const STATE_CONNECTED: u8 = 2;
const STATE_RECONNECTING: u8 = 3;

pub struct ObsClient {
    #[allow(dead_code)]
    pub(crate) inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    endpoint: String,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    supervisor: Arc<std::sync::Mutex<Option<JoinHandle<()>>>>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
}

impl ObsClient {
    pub async fn connect(endpoint: &str, password: Option<&str>) -> Result<Self, ObsError> {
        let (host, port) = parse_endpoint(endpoint)?;

        let inner = Arc::new(tokio::sync::RwLock::new(None::<obws::Client>));
        let state = Arc::new(AtomicU8::new(STATE_CONNECTING));
        let shutdown = Arc::new(Notify::new());
        let connected_at = Arc::new(RwLock::new(None::<OffsetDateTime>));

        let handle = tokio::spawn(run_supervisor(
            host,
            port,
            password.map(str::to_owned),
            Arc::clone(&inner),
            Arc::clone(&state),
            Arc::clone(&shutdown),
            Arc::clone(&connected_at),
        ));

        Ok(Self {
            inner,
            endpoint: endpoint.to_owned(),
            state,
            shutdown,
            supervisor: Arc::new(std::sync::Mutex::new(Some(handle))),
            connected_at,
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn connected_at(&self) -> Option<OffsetDateTime> {
        self.connected_at.read().ok().and_then(|g| *g)
    }

    pub fn connection_state(&self) -> ConnectionState {
        match self.state.load(Ordering::Acquire) {
            STATE_CONNECTED => ConnectionState::Connected,
            STATE_CONNECTING => ConnectionState::Connecting,
            STATE_RECONNECTING => ConnectionState::Reconnecting,
            _ => ConnectionState::Disconnected,
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_one();
        let handle = self.supervisor.lock().ok().and_then(|mut g| g.take());
        if let Some(h) = handle {
            let _ = h.await;
        }
    }

    #[cfg(test)]
    pub fn new_for_test(endpoint: String) -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(None)),
            endpoint,
            state: Arc::new(AtomicU8::new(STATE_DISCONNECTED)),
            shutdown: Arc::new(Notify::new()),
            supervisor: Arc::new(std::sync::Mutex::new(None)),
            connected_at: Arc::new(RwLock::new(None)),
        }
    }
}

/// Returns `min(2^attempt seconds, 60 seconds) + random jitter up to 10% of the base.
pub(crate) fn compute_backoff(attempt: u32) -> Duration {
    let base_secs = (1u64 << attempt.min(6)).min(60);
    let max_jitter_ms = base_secs * 100;
    let jitter_ms = rand::rng().random_range(0..=max_jitter_ms);
    Duration::from_millis(base_secs * 1000 + jitter_ms)
}

async fn run_supervisor(
    host: String,
    port: u16,
    password: Option<String>,
    inner: Arc<tokio::sync::RwLock<Option<obws::Client>>>,
    state: Arc<AtomicU8>,
    shutdown: Arc<Notify>,
    connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
) {
    let mut attempt: u32 = 0;

    loop {
        if attempt > 0 {
            let delay = compute_backoff(attempt - 1);
            tracing::info!(
                host = %host,
                port,
                attempt,
                delay_ms = delay.as_millis(),
                "reconnecting to OBS"
            );
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = shutdown.notified() => {
                    state.store(STATE_DISCONNECTED, Ordering::Release);
                    return;
                }
            }
        }

        let conn_state = if attempt == 0 {
            STATE_CONNECTING
        } else {
            STATE_RECONNECTING
        };
        state.store(conn_state, Ordering::Release);
        tracing::debug!(host = %host, port, attempt, "attempting OBS connection");

        match obws::Client::connect(&host, port, password.as_deref())
            .await
            .map_err(map_obws_error)
        {
            Ok(client) => {
                let events = client.events();
                inner.write().await.replace(client);

                if let Ok(mut g) = connected_at.write() {
                    *g = Some(OffsetDateTime::now_utc());
                }

                state.store(STATE_CONNECTED, Ordering::Release);
                tracing::info!(host = %host, port, "connected to OBS");

                match events {
                    Ok(mut stream) => loop {
                        tokio::select! {
                            () = shutdown.notified() => {
                                inner.write().await.take();
                                state.store(STATE_DISCONNECTED, Ordering::Release);
                                tracing::info!("OBS supervisor shutting down");
                                return;
                            }
                            item = stream.next() => {
                                if item.is_none() {
                                    tracing::info!(host = %host, port, "OBS connection lost; reconnecting");
                                    break;
                                }
                            }
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "OBS event subscription unavailable; waiting for shutdown only"
                        );
                        shutdown.notified().await;
                        inner.write().await.take();
                        state.store(STATE_DISCONNECTED, Ordering::Release);
                        return;
                    }
                }

                inner.write().await.take();
                attempt = 1;
            }

            Err(ObsError::Authentication) => {
                tracing::warn!(host = %host, port, "OBS authentication rejected");
                state.store(STATE_DISCONNECTED, Ordering::Release);
                return;
            }

            Err(e) => {
                tracing::debug!(host = %host, port, attempt, error = %e, "OBS connection attempt failed");
                attempt = attempt.saturating_add(1);
            }
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
    fn connection_state_disconnected_for_test_constructor() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        assert_eq!(client.connection_state(), ConnectionState::Disconnected);
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

    #[test]
    fn compute_backoff_caps_at_60s_for_attempt_six() {
        let d = compute_backoff(6);
        assert!(d.as_secs() >= 60);
    }

    #[test]
    fn compute_backoff_caps_at_60s_for_attempt_seven() {
        let d = compute_backoff(7);
        assert!(d.as_secs() >= 60);
    }

    #[test]
    fn compute_backoff_first_attempt_under_two_seconds() {
        let d = compute_backoff(0);
        assert!(d.as_millis() < 2_000);
    }

    #[test]
    fn compute_backoff_attempt_five_under_60s() {
        let d = compute_backoff(5);
        assert!(d.as_secs() < 60);
    }
}
