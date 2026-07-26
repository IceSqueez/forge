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
