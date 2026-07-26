use std::time::{Duration, Instant};

use crate::client::map_obws_error;
use crate::error::ObsError;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ObsProbeResult {
    pub obs_version: String,
    pub obs_websocket_version: String,
    pub scene_count: usize,
    pub round_trip_ms: u64,
}

pub async fn probe_connection(
    host: &str,
    port: u16,
    password: &str,
) -> Result<ObsProbeResult, ObsError> {
    tokio::time::timeout(PROBE_TIMEOUT, run_probe(host, port, password))
        .await
        .unwrap_or(Err(ObsError::Timeout))
}

async fn run_probe(host: &str, port: u16, password: &str) -> Result<ObsProbeResult, ObsError> {
    let password = (!password.is_empty()).then_some(password);
    let connect_config = obws::client::ConnectConfig {
        host,
        port,
        password,
        event_subscriptions: Some(obws::requests::EventSubscription::NONE),
        broadcast_capacity: obws::client::DEFAULT_BROADCAST_CAPACITY,
        connect_timeout: PROBE_TIMEOUT,
        dangerous: None,
    };

    let mut client = obws::Client::connect_with_config(connect_config)
        .await
        .map_err(map_obws_error)?;

    let started = Instant::now();
    let version = client.general().version().await.map_err(map_obws_error)?;
    let round_trip_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    let scenes = client
        .scenes()
        .list()
        .await
        .map_err(map_obws_error)?
        .scenes
        .len();

    client.disconnect().await;

    Ok(ObsProbeResult {
        obs_version: version.obs_studio_version.to_string(),
        obs_websocket_version: version.obs_web_socket_version.to_string(),
        scene_count: scenes,
        round_trip_ms,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    const PROBE_PASSWORD: &str = "obs-probe-secret-1a2b3c";

    /// Binds and immediately releases a loopback port so the probe hits a refused connection
    /// without touching anything outside this process.
    async fn closed_loopback_port() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    async fn probe_error(port: u16, password: &str) -> ObsError {
        match probe_connection("127.0.0.1", port, password).await {
            Err(e) => e,
            Ok(_) => panic!("probe unexpectedly reached an obs-websocket server on port {port}"),
        }
    }

    #[tokio::test]
    async fn probe_of_a_port_nothing_listens_on_reports_a_connect_failure() {
        let port = closed_loopback_port().await;
        let error = probe_error(port, "").await;
        assert!(
            matches!(error, ObsError::Connect(_)),
            "expected a connect failure, got {error:?}"
        );
    }

    // Why: the probe is the only path that takes a plaintext OBS password straight from a form
    // field, and its failure text is rendered verbatim in the setup screen banner.
    #[tokio::test]
    async fn probe_failure_never_carries_the_password_into_its_error_text() {
        let port = closed_loopback_port().await;
        let error = probe_error(port, PROBE_PASSWORD).await;
        assert!(
            !format!("{error}").contains(PROBE_PASSWORD),
            "probe error leaked the password: {error}"
        );
        assert!(
            !format!("{error:?}").contains(PROBE_PASSWORD),
            "probe error Debug leaked the password"
        );
    }
}
