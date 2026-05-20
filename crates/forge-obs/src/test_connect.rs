use std::time::Instant;

use crate::error::ObsError;

/// Summary returned from a successful one-shot connection check.
#[derive(Debug, Clone)]
pub struct ObsServerInfo {
    pub obs_websocket_version: String,
    pub scene_count: usize,
    pub rtt_ms: u32,
}

fn map_err(e: obws::error::Error) -> ObsError {
    use obws::error::Error as ObwsError;
    match e {
        ObwsError::Connect(c) => ObsError::Connect(c.to_string()),
        ObwsError::Handshake(_) => ObsError::Authentication,
        ObwsError::Timeout => ObsError::Timeout,
        ObwsError::Disconnected => ObsError::Disconnected,
        other => ObsError::Protocol(other.to_string()),
    }
}

/// One-shot connection check used by the "Test connection" button on the OBS
/// disconnected panel. Connects, fetches version + scene list, returns a
/// summary. Drops the connection on completion — does not promote it to a
/// long-lived session.
pub async fn test_connect(
    host: &str,
    port: u16,
    password: Option<&str>,
) -> Result<ObsServerInfo, ObsError> {
    let start = Instant::now();
    let client = obws::Client::connect(host, port, password)
        .await
        .map_err(map_err)?;

    let version = client.general().version().await.map_err(map_err)?;
    let rtt_ms = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);

    let scenes = client.scenes().list().await.map_err(map_err)?;

    Ok(ObsServerInfo {
        obs_websocket_version: version.obs_web_socket_version.to_string(),
        scene_count: scenes.scenes.len(),
        rtt_ms,
    })
}
