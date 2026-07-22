use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{AtomicConnectionState, Backoff, ConnectionState, HealthDelta};
use forge_storage::CredentialsRepo;

use crate::auth::AuthState;
use crate::client::VtsWs;
use crate::error::VTubeError;
use crate::health::{HealthSnapshot, update_from_event};
use crate::protocol::new_request;
use crate::request::PendingRequest;

const VTS_BACKOFF_CAP: Duration = Duration::from_secs(30);

fn emit_connection_changed(
    publisher: &dyn EventPublisher,
    endpoint: &str,
    connected: bool,
    reason: Option<String>,
) {
    let reason_val = reason
        .map(serde_json::Value::String)
        .unwrap_or(serde_json::Value::Null);
    let payload = serde_json::json!({
        "connected": connected,
        "endpoint": endpoint,
        "reason": reason_val,
    });
    publisher.publish(Event::new(
        EventSource::VTube,
        "vtube.connection.changed",
        payload,
    ));
}

async fn send_ws_msg<T: serde::Serialize>(ws: &mut VtsWs, msg: &T) -> Result<(), VTubeError> {
    let text = serde_json::to_string(msg).map_err(VTubeError::Json)?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| VTubeError::Connect(e.to_string()))
}

async fn recv_next_text(ws: &mut VtsWs) -> Result<serde_json::Value, VTubeError> {
    loop {
        match ws.next().await {
            None => return Err(VTubeError::Connect("connection closed".to_owned())),
            Some(Err(e)) => return Err(VTubeError::Connect(e.to_string())),
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text).map_err(VTubeError::Json);
            }
            Some(Ok(_)) => {}
        }
    }
}

async fn request_new_token(ws: &mut VtsWs, endpoint: &str) -> Result<String, VTubeError> {
    let req = new_request(
        "AuthenticationTokenRequest",
        serde_json::json!({ "pluginName": "forge", "pluginDeveloper": "forge" }),
    );
    send_ws_msg(ws, &req).await?;
    tracing::debug!(endpoint, "sent AuthenticationTokenRequest, awaiting popup");

    let msg = tokio::time::timeout(Duration::from_secs(30), recv_next_text(ws))
        .await
        .map_err(|_| VTubeError::TokenTimeout)??;

    let msg_type = msg["messageType"].as_str().unwrap_or("");
    if msg_type != "AuthenticationTokenResponse" {
        return Err(VTubeError::Request {
            message: format!("expected AuthenticationTokenResponse, got {msg_type}"),
        });
    }

    let granted = msg["data"]["granted"].as_bool().unwrap_or(false);
    if !granted {
        return Err(VTubeError::TokenDenied);
    }

    msg["data"]["authenticationToken"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| VTubeError::Request {
            message: "authenticationToken missing in response".to_owned(),
        })
}

async fn authenticate_with_token(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    token: &str,
    endpoint: &str,
) -> Result<(), VTubeError> {
    let req = new_request(
        "AuthenticationRequest",
        serde_json::json!({
            "pluginName": "forge",
            "pluginDeveloper": "forge",
            "authenticationToken": token
        }),
    );
    send_ws_msg(ws, &req).await?;
    tracing::debug!(endpoint, "sent AuthenticationRequest");

    let msg = recv_next_text(ws).await?;

    let msg_type = msg["messageType"].as_str().unwrap_or("");
    if msg_type != "AuthenticationResponse" {
        return Err(VTubeError::Request {
            message: format!("expected AuthenticationResponse, got {msg_type}"),
        });
    }

    let authenticated = msg["data"]["authenticated"].as_bool().unwrap_or(false);
    if !authenticated {
        let _ = crate::credentials::clear(creds).await;
        tracing::warn!(
            endpoint,
            "VTube Studio token rejected; cleared stored credential"
        );
        return Err(VTubeError::TokenRejected);
    }

    Ok(())
}

async fn run_auth(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    endpoint: &str,
) -> Result<(), VTubeError> {
    let stored = crate::credentials::load(creds).await.ok().flatten();

    let token = if let Some(c) = stored {
        tracing::debug!(endpoint, "using stored VTube Studio credential");
        c.token
    } else {
        let new_token = request_new_token(ws, endpoint).await?;
        if let Err(e) = crate::credentials::store(creds, &new_token, "1.0").await {
            tracing::warn!(endpoint, error = %e, "failed to persist VTube Studio token");
        }
        new_token
    };

    authenticate_with_token(ws, creds, &token, endpoint).await
}

pub(crate) struct SupervisorContext {
    pub(crate) endpoint: String,
    pub(crate) state: Arc<AtomicConnectionState>,
    pub(crate) auth_state: Arc<RwLock<AuthState>>,
    pub(crate) shutdown: Arc<Notify>,
    pub(crate) connected_at: Arc<RwLock<Option<OffsetDateTime>>>,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) req_rx: mpsc::UnboundedReceiver<PendingRequest>,
    pub(crate) health_state: Arc<RwLock<HealthSnapshot>>,
    pub(crate) health_tx: broadcast::Sender<HealthDelta>,
    pub(crate) content_notifier: crate::content::ContentNotifier,
}

pub(crate) async fn run_supervisor(ctx: SupervisorContext) {
    let SupervisorContext {
        endpoint,
        state,
        auth_state,
        shutdown,
        connected_at,
        publisher,
        creds,
        mut req_rx,
        health_state,
        health_tx,
        content_notifier,
    } = ctx;

    let mut backoff = Backoff::with_cap(VTS_BACKOFF_CAP);
    let mut reconnecting = false;

    loop {
        if reconnecting {
            let delay = backoff.next_delay();
            tracing::info!(
                endpoint = %endpoint,
                delay_ms = delay.as_millis(),
                "reconnecting to VTube Studio"
            );
            tokio::select! {
                () = tokio::time::sleep(delay) => {}
                () = shutdown.notified() => {
                    state.store(ConnectionState::Disconnected);
                    emit_connection_changed(&*publisher, &endpoint, false, None);
                    return;
                }
            }
        }

        state.store(if reconnecting {
            ConnectionState::Reconnecting
        } else {
            ConnectionState::Connecting
        });
        tracing::debug!(endpoint = %endpoint, "attempting VTube Studio connection");

        let mut ws = match tokio_tungstenite::connect_async(&endpoint).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "VTube Studio connection attempt failed"
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
                reconnecting = true;
                continue;
            }
        };

        match run_auth(&mut ws, &*creds, &endpoint).await {
            Ok(()) => {}
            Err(VTubeError::TokenRejected) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("auth_required".to_owned()),
                );
                state.store(ConnectionState::Disconnected);
                return;
            }
            Err(VTubeError::TokenDenied | VTubeError::TokenTimeout) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("auth_denied".to_owned()),
                );
                state.store(ConnectionState::Disconnected);
                return;
            }
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "auth failed, will retry"
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
                reconnecting = true;
                continue;
            }
        }

        if let Err(e) = crate::events::subscribe_all(&mut ws).await {
            tracing::debug!(endpoint = %endpoint, error = %e, "event subscription failed, will retry");
            emit_connection_changed(&*publisher, &endpoint, false, Some(e.to_string()));
            reconnecting = true;
            continue;
        }

        if let Ok(mut g) = connected_at.write() {
            *g = Some(OffsetDateTime::now_utc());
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Connected;
        }
        state.store(ConnectionState::Connected);
        emit_connection_changed(&*publisher, &endpoint, true, None);
        tracing::info!(endpoint = %endpoint, "connected and authenticated to VTube Studio");

        let mut pending: HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>> =
            HashMap::new();

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    state.store(ConnectionState::Disconnected);
                    if let Ok(mut g) = auth_state.write() {
                        *g = AuthState::Cold;
                    }
                    emit_connection_changed(&*publisher, &endpoint, false, None);
                    return;
                }
                Some(req) = req_rx.recv() => {
                    if ws
                        .send(Message::Text(req.payload.into()))
                        .await
                        .is_ok()
                    {
                        pending.insert(req.request_id, req.respond_to);
                    }
                }
                msg = ws.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            tracing::info!(endpoint = %endpoint, "VTube Studio connection closed");
                            pending.clear();
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(val) =
                                serde_json::from_str::<serde_json::Value>(&text)
                            {
                                let msg_type = val["messageType"].as_str().unwrap_or("");
                                if msg_type.ends_with("Response") {
                                    let req_id =
                                        val["requestID"].as_str().unwrap_or("").to_owned();
                                    if let Some(tx) = pending.remove(&req_id) {
                                        let _ = tx.send(val["data"].clone());
                                    }
                                } else if let Ok(env) = serde_json::from_value::<
                                    crate::events::RawEnvelope,
                                >(val)
                                {
                                    if env.message_type == "ModelLoadedEvent" {
                                        content_notifier.notify_model_changed();
                                    }
                                    crate::events::dispatch_vts_event(&env, &*publisher);
                                    update_from_event(&env, &health_state, &health_tx);
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        if let Ok(mut g) = connected_at.write() {
            *g = None;
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Cold;
        }
        emit_connection_changed(&*publisher, &endpoint, false, None);
        backoff.reset();
        reconnecting = true;
    }
}
