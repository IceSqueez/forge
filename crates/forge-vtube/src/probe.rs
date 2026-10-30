use std::time::{Duration, Instant};

use crate::error::VTubeError;
use crate::vts::endpoint::endpoint_url;
use crate::vts::message_type::{API_STATE_REQUEST, API_STATE_RESPONSE};
use crate::vts::protocol::new_request;
use crate::vts::socket::{dial, recv_next_text, send_ws_msg};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct VTubeProbeResult {
    pub api_version: String,
    pub already_authenticated: bool,
    pub round_trip_ms: u64,
}

pub async fn probe_connection(host: &str, port: u16) -> Result<VTubeProbeResult, VTubeError> {
    tokio::time::timeout(PROBE_TIMEOUT, run_probe(host, port))
        .await
        .unwrap_or(Err(VTubeError::Timeout))
}

async fn run_probe(host: &str, port: u16) -> Result<VTubeProbeResult, VTubeError> {
    let mut ws = dial(&endpoint_url(host, port))
        .await
        .map_err(|failed| VTubeError::Connect(failed.detail))?;

    let started = Instant::now();
    let req = new_request(API_STATE_REQUEST, serde_json::json!({}));
    send_ws_msg(&mut ws, &req).await?;
    let msg = recv_next_text(&mut ws).await?;
    let round_trip_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    if let Err(error) = ws.close(None).await {
        tracing::debug!(%error, "probe socket close failed");
    }

    let msg_type = msg["messageType"].as_str().unwrap_or("");
    if msg_type != API_STATE_RESPONSE {
        return Err(VTubeError::Request {
            message: format!("expected {API_STATE_RESPONSE}, got {msg_type}"),
        });
    }

    let api_version = msg["data"]["vTubeStudioVersion"]
        .as_str()
        .unwrap_or("unknown")
        .to_owned();
    let already_authenticated = msg["data"]["currentSessionAuthenticated"]
        .as_bool()
        .unwrap_or(false);

    Ok(VTubeProbeResult {
        api_version,
        already_authenticated,
        round_trip_ms,
    })
}
