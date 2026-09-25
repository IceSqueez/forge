use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, mpsc, oneshot};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;

use forge_events::{Event, EventPublisher, EventSource};
use forge_platform_core::{
    AtomicConnectionState, Backoff, ConnectionState, HealthDelta, connection_state_changed_event,
};
use forge_storage::CredentialsRepo;

use crate::auth::AuthState;
use crate::client::{VTUBE_PLATFORM_ID, VtsWs};
use crate::error::VTubeError;
use crate::health::{
    HealthSnapshot, apply_model, apply_tracking, clear_session_state, update_from_event,
};
use crate::payload_fields::connection as connection_fields;
use crate::payload_fields::expression as expression_fields;
use crate::protocol::{
    API_ERROR_MESSAGE_TYPE, TOKEN_REQUEST_DENIED_ERROR_ID, check_response, is_reply, new_request,
};
use crate::request::{PendingRequest, REQUEST_TIMEOUT};

const VTS_BACKOFF_CAP: Duration = Duration::from_secs(30);
const EXPRESSION_POLL_INTERVAL: Duration = Duration::from_secs(3);
const EXPRESSION_POLL_TIMEOUT: Duration = Duration::from_secs(5);
const UNANSWERED_POLLS_BEFORE_SUSPECT: u32 = 2;
const REQUEST_SWEEP_INTERVAL: Duration = Duration::from_secs(1);
const TOKEN_APPROVAL_TIMEOUT: Duration = Duration::from_secs(30);
const HANDSHAKE_REPLY_TIMEOUT: Duration = Duration::from_secs(10);

const REASON_SOCKET_CLOSED: &str = "socket_closed";
const REASON_UNRESPONSIVE: &str = "unresponsive";

struct InFlight {
    respond_to: oneshot::Sender<serde_json::Value>,
    sent_at: Instant,
}

enum SeedQuery {
    CurrentModel,
    FaceFound,
}

struct ExpressionPoll {
    request_id: String,
    reply: oneshot::Receiver<serde_json::Value>,
    deadline: Instant,
}

fn emit_connection_changed(
    publisher: &dyn EventPublisher,
    endpoint: &str,
    is_connected: bool,
    reason: Option<&str>,
    detail: Option<String>,
) {
    let payload = serde_json::json!({
        (connection_fields::IS_CONNECTED): is_connected,
        (connection_fields::ENDPOINT): endpoint,
        (connection_fields::REASON): reason,
        (connection_fields::DETAIL): detail,
    });
    publisher.publish(Event::new(
        EventSource::VTube,
        "vtube.connection.changed",
        payload,
    ));
}

fn snapshot_expressions(data: &serde_json::Value) -> HashMap<String, bool> {
    data["expressions"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let file = e["file"].as_str()?.to_owned();
                    let active = e["active"].as_bool().unwrap_or(false);
                    Some((file, active))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn diff_and_emit_expressions(
    data: &serde_json::Value,
    baseline: &mut Option<HashMap<String, bool>>,
    publisher: &dyn EventPublisher,
) {
    let current = snapshot_expressions(data);
    if let Some(prev) = baseline {
        for (file, is_active) in &current {
            if prev
                .get(file)
                .is_some_and(|prev_active| prev_active != is_active)
            {
                publisher.publish(Event::new(
                    EventSource::VTube,
                    "vtube.expression.state_changed",
                    serde_json::json!({
                        (expression_fields::EXPRESSION_FILE): file,
                        (expression_fields::IS_ACTIVE): is_active,
                    }),
                ));
            }
        }
    }
    *baseline = Some(current);
}

async fn await_expression_poll(poll: &mut Option<ExpressionPoll>) -> Option<serde_json::Value> {
    match poll.as_mut() {
        Some(poll) => match tokio::time::timeout_at(poll.deadline, &mut poll.reply).await {
            Ok(Ok(data)) => Some(data),
            _ => None,
        },
        None => std::future::pending().await,
    }
}

pub(crate) async fn send_ws_msg<T: serde::Serialize>(
    ws: &mut VtsWs,
    msg: &T,
) -> Result<(), VTubeError> {
    let text = serde_json::to_string(msg).map_err(VTubeError::Json)?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| VTubeError::Connect(e.to_string()))
}

pub(crate) async fn recv_next_text(ws: &mut VtsWs) -> Result<serde_json::Value, VTubeError> {
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

async fn recv_reply(ws: &mut VtsWs, request_id: &str) -> Result<serde_json::Value, VTubeError> {
    loop {
        let msg = recv_next_text(ws).await?;
        if msg["requestID"].as_str() == Some(request_id) {
            return Ok(msg);
        }
    }
}

async fn exchange(
    ws: &mut VtsWs,
    msg_type: &str,
    data: serde_json::Value,
    limit: Duration,
) -> Result<serde_json::Value, VTubeError> {
    let req = new_request(msg_type, data);
    send_ws_msg(ws, &req).await?;
    tokio::time::timeout(limit, recv_reply(ws, &req.request_id))
        .await
        .map_err(|_| VTubeError::Timeout)?
}

fn api_error_id(msg: &serde_json::Value) -> Option<i64> {
    msg["data"]["errorID"].as_i64()
}

fn api_error(msg: &serde_json::Value) -> VTubeError {
    match check_response(&msg["data"]) {
        Err(e) => e,
        Ok(()) => VTubeError::Request {
            message: "VTube Studio returned an error without an errorID".to_owned(),
        },
    }
}

fn unexpected_reply(expected: &str, msg: &serde_json::Value) -> VTubeError {
    let got = msg["messageType"].as_str().unwrap_or("");
    VTubeError::Request {
        message: format!("expected {expected}, got {got}"),
    }
}

async fn request_new_token(ws: &mut VtsWs, endpoint: &str) -> Result<String, VTubeError> {
    tracing::debug!(
        endpoint,
        "sending AuthenticationTokenRequest, awaiting popup"
    );
    let msg = exchange(
        ws,
        "AuthenticationTokenRequest",
        serde_json::json!({
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME
        }),
        TOKEN_APPROVAL_TIMEOUT,
    )
    .await
    .map_err(|e| match e {
        VTubeError::Timeout => VTubeError::TokenTimeout,
        other => other,
    })?;

    match msg["messageType"].as_str().unwrap_or("") {
        "AuthenticationTokenResponse" => msg["data"]["authenticationToken"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| VTubeError::Request {
                message: "authenticationToken missing in response".to_owned(),
            }),
        API_ERROR_MESSAGE_TYPE if api_error_id(&msg) == Some(TOKEN_REQUEST_DENIED_ERROR_ID) => {
            Err(VTubeError::TokenDenied)
        }
        API_ERROR_MESSAGE_TYPE => Err(api_error(&msg)),
        _ => Err(unexpected_reply("AuthenticationTokenResponse", &msg)),
    }
}

async fn authenticate_with_token(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    token: &str,
    endpoint: &str,
) -> Result<(), VTubeError> {
    tracing::debug!(endpoint, "sending AuthenticationRequest");
    let msg = exchange(
        ws,
        "AuthenticationRequest",
        serde_json::json!({
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME,
            "authenticationToken": token
        }),
        HANDSHAKE_REPLY_TIMEOUT,
    )
    .await?;

    let rejection = match msg["messageType"].as_str().unwrap_or("") {
        "AuthenticationResponse" if msg["data"]["authenticated"].as_bool() == Some(true) => {
            return Ok(());
        }
        "AuthenticationResponse" => msg["data"]["reason"].as_str().unwrap_or("").to_owned(),
        API_ERROR_MESSAGE_TYPE if api_error_id(&msg) == Some(TOKEN_REQUEST_DENIED_ERROR_ID) => {
            msg["data"]["message"].as_str().unwrap_or("").to_owned()
        }
        API_ERROR_MESSAGE_TYPE => return Err(api_error(&msg)),
        _ => return Err(unexpected_reply("AuthenticationResponse", &msg)),
    };

    let _ = crate::credentials::clear(creds).await;
    tracing::warn!(
        endpoint,
        reason = %rejection,
        "VTube Studio token rejected; cleared stored credential"
    );
    Err(VTubeError::TokenRejected)
}

async fn run_auth(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    endpoint: &str,
    auth_state: &RwLock<AuthState>,
    publisher: &dyn EventPublisher,
) -> Result<(), VTubeError> {
    let stored = crate::credentials::load(creds).await.ok().flatten();

    let token = if let Some(c) = stored {
        tracing::debug!(endpoint, "using stored VTube Studio credential");
        c.token
    } else {
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::AwaitingApproval;
        }
        emit_connection_changed(publisher, endpoint, false, Some("awaiting_approval"), None);
        request_new_token(ws, endpoint).await?
    };

    authenticate_with_token(ws, creds, &token, endpoint).await?;

    let (host, port) = crate::client::split_endpoint(endpoint);
    if let Err(e) = crate::credentials::store(creds, &token, "1.0", &host, port).await {
        tracing::warn!(endpoint, error = %e, "failed to persist VTube Studio credential");
    }
    Ok(())
}

fn connection_state_transition(previous: ConnectionState, next: ConnectionState) -> Option<Event> {
    (previous != next).then(|| connection_state_changed_event(VTUBE_PLATFORM_ID, next))
}

pub(crate) fn set_connection_state(
    state: &AtomicConnectionState,
    health_state: &RwLock<HealthSnapshot>,
    publisher: &dyn EventPublisher,
    new_state: ConnectionState,
) {
    let previous = state.load();
    state.store(new_state);
    let dialing = matches!(
        new_state,
        ConnectionState::Connecting | ConnectionState::Reconnecting
    );
    if let Ok(mut g) = health_state.write() {
        g.dialing = dialing;
    }
    if let Some(event) = connection_state_transition(previous, new_state) {
        publisher.publish(event);
    }
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
    pub(crate) connected_notifier: mpsc::UnboundedSender<()>,
    pub(crate) auto_reconnect: Arc<AtomicBool>,
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
        connected_notifier,
        auto_reconnect,
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
                    set_connection_state(&state, &health_state, &*publisher, ConnectionState::Disconnected);
                    emit_connection_changed(&*publisher, &endpoint, false, None, None);
                    return;
                }
            }
            if !auto_reconnect.load(Ordering::Relaxed) {
                set_connection_state(
                    &state,
                    &health_state,
                    &*publisher,
                    ConnectionState::Disconnected,
                );
                emit_connection_changed(&*publisher, &endpoint, false, None, None);
                return;
            }
        }

        set_connection_state(
            &state,
            &health_state,
            &*publisher,
            if reconnecting {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Connecting
            },
        );
        tracing::debug!(endpoint = %endpoint, "attempting VTube Studio connection");

        let connect_attempt = tokio::select! {
            outcome = tokio_tungstenite::connect_async(&endpoint) => outcome,
            () = shutdown.notified() => {
                set_connection_state(&state, &health_state, &*publisher, ConnectionState::Disconnected);
                emit_connection_changed(&*publisher, &endpoint, false, None, None);
                return;
            }
        };

        let mut ws = match connect_attempt {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "VTube Studio connection attempt failed"
                );
                let retry = auto_reconnect.load(Ordering::Relaxed);
                if !retry {
                    set_connection_state(
                        &state,
                        &health_state,
                        &*publisher,
                        ConnectionState::Disconnected,
                    );
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("connect_failed"),
                    Some(e.to_string()),
                );
                if retry {
                    reconnecting = true;
                    continue;
                }
                return;
            }
        };

        let auth_outcome = tokio::select! {
            outcome = run_auth(&mut ws, &*creds, &endpoint, &auth_state, &*publisher) => outcome,
            () = shutdown.notified() => {
                announce_stopped(&state, &health_state, &*publisher, &auth_state, &endpoint);
                return;
            }
        };
        match auth_outcome {
            Ok(()) => {}
            Err(VTubeError::TokenRejected) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(
                    &state,
                    &health_state,
                    &*publisher,
                    ConnectionState::Disconnected,
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some("auth_required"), None);
                return;
            }
            Err(VTubeError::TokenDenied) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(
                    &state,
                    &health_state,
                    &*publisher,
                    ConnectionState::Disconnected,
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some("auth_denied"), None);
                return;
            }
            Err(VTubeError::TokenTimeout) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(
                    &state,
                    &health_state,
                    &*publisher,
                    ConnectionState::Disconnected,
                );
                emit_connection_changed(&*publisher, &endpoint, false, Some("auth_timeout"), None);
                return;
            }
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "auth failed, will retry"
                );
                let retry = auto_reconnect.load(Ordering::Relaxed);
                if !retry {
                    set_connection_state(
                        &state,
                        &health_state,
                        &*publisher,
                        ConnectionState::Disconnected,
                    );
                }
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some("auth_failed"),
                    Some(e.to_string()),
                );
                if retry {
                    reconnecting = true;
                    continue;
                }
                return;
            }
        }

        let subscribe_outcome = tokio::select! {
            outcome = tokio::time::timeout(HANDSHAKE_REPLY_TIMEOUT, crate::events::subscribe_all(&mut ws)) => {
                outcome.unwrap_or(Err(VTubeError::Timeout))
            }
            () = shutdown.notified() => {
                announce_stopped(&state, &health_state, &*publisher, &auth_state, &endpoint);
                return;
            }
        };
        if let Err(e) = subscribe_outcome {
            tracing::debug!(endpoint = %endpoint, error = %e, "event subscription failed, will retry");
            let retry = auto_reconnect.load(Ordering::Relaxed);
            if !retry {
                set_connection_state(
                    &state,
                    &health_state,
                    &*publisher,
                    ConnectionState::Disconnected,
                );
            }
            emit_connection_changed(
                &*publisher,
                &endpoint,
                false,
                Some("subscribe_failed"),
                Some(e.to_string()),
            );
            if retry {
                reconnecting = true;
                continue;
            }
            return;
        }

        if let Ok(mut g) = connected_at.write() {
            *g = Some(OffsetDateTime::now_utc());
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Connected;
        }
        set_connection_state(
            &state,
            &health_state,
            &*publisher,
            ConnectionState::Connected,
        );
        emit_connection_changed(&*publisher, &endpoint, true, None, None);
        let _ = connected_notifier.send(());
        tracing::info!(endpoint = %endpoint, "connected and authenticated to VTube Studio");

        let mut pending: HashMap<String, InFlight> = HashMap::new();
        let mut seeds: HashMap<String, SeedQuery> = HashMap::new();
        let mut last_face_found: Option<bool> = None;
        let mut expr_tick = tokio::time::interval(EXPRESSION_POLL_INTERVAL);
        let mut sweep_tick = tokio::time::interval(REQUEST_SWEEP_INTERVAL);
        let mut expr_baseline: Option<HashMap<String, bool>> = None;
        let mut expr_poll: Option<ExpressionPoll> = None;
        let mut unanswered_polls: u32 = 0;
        let mut close_reason = REASON_SOCKET_CLOSED;

        for (msg_type, query) in [
            ("CurrentModelRequest", SeedQuery::CurrentModel),
            ("FaceFoundRequest", SeedQuery::FaceFound),
        ] {
            let req = new_request(msg_type, serde_json::json!({}));
            if send_ws_msg(&mut ws, &req).await.is_ok() {
                seeds.insert(req.request_id, query);
            }
        }
        content_notifier.notify_model_changed();

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    clear_session_state(&health_state, &health_tx);
                    announce_stopped(&state, &health_state, &*publisher, &auth_state, &endpoint);
                    return;
                }
                Some(req) = req_rx.recv() => {
                    if ws
                        .send(Message::Text(req.payload.into()))
                        .await
                        .is_ok()
                    {
                        pending.insert(
                            req.request_id,
                            InFlight {
                                respond_to: req.respond_to,
                                sent_at: Instant::now(),
                            },
                        );
                    }
                }
                msg = ws.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            tracing::info!(endpoint = %endpoint, "VTube Studio connection closed");
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else {
                                continue;
                            };
                            let msg_type = val["messageType"].as_str().unwrap_or("");
                            if is_reply(msg_type) {
                                let req_id = val["requestID"].as_str().unwrap_or("");
                                if let Some(in_flight) = pending.remove(req_id) {
                                    let _ = in_flight.respond_to.send(val["data"].clone());
                                } else if let Some(query) = seeds.remove(req_id) {
                                    apply_seed(
                                        &query,
                                        &val["data"],
                                        &mut last_face_found,
                                        &health_state,
                                        &health_tx,
                                    );
                                } else if msg_type == API_ERROR_MESSAGE_TYPE {
                                    tracing::debug!(
                                        endpoint = %endpoint,
                                        error_id = ?api_error_id(&val),
                                        "VTube Studio error for a request nobody awaits"
                                    );
                                }
                            } else if let Ok(env) =
                                serde_json::from_value::<crate::events::RawEnvelope>(val)
                            {
                                if env.message_type == "ModelLoadedEvent" {
                                    content_notifier.notify_model_changed();
                                    expr_baseline = None;
                                }
                                if !is_repeated_face_state(&env, &mut last_face_found) {
                                    crate::events::dispatch_vts_event(&env, &*publisher);
                                }
                                update_from_event(&env, &health_state, &health_tx);
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
                _ = expr_tick.tick(), if expr_poll.is_none() => {
                    let req = new_request("ExpressionStateRequest", serde_json::json!({ "details": false }));
                    if let Ok(text) = serde_json::to_string(&req) {
                        let (tx, rx) = oneshot::channel();
                        if ws.send(Message::Text(text.into())).await.is_ok() {
                            let sent_at = Instant::now();
                            pending.insert(
                                req.request_id.clone(),
                                InFlight { respond_to: tx, sent_at },
                            );
                            expr_poll = Some(ExpressionPoll {
                                request_id: req.request_id,
                                reply: rx,
                                deadline: sent_at + EXPRESSION_POLL_TIMEOUT,
                            });
                        }
                    }
                }
                result = await_expression_poll(&mut expr_poll) => {
                    let finished = expr_poll.take();
                    if let Some(data) = result {
                        unanswered_polls = 0;
                        if check_response(&data).is_ok() {
                            diff_and_emit_expressions(&data, &mut expr_baseline, &*publisher);
                        }
                    } else {
                        if let Some(poll) = finished {
                            pending.remove(&poll.request_id);
                        }
                        unanswered_polls += 1;
                        if unanswered_polls >= UNANSWERED_POLLS_BEFORE_SUSPECT {
                            tracing::warn!(
                                endpoint = %endpoint,
                                "VTube Studio stopped answering the expression poll; dropping the session"
                            );
                            close_reason = REASON_UNRESPONSIVE;
                            break;
                        }
                    }
                }
                _ = sweep_tick.tick() => {
                    if pending.values().any(|f| f.sent_at.elapsed() >= REQUEST_TIMEOUT) {
                        tracing::warn!(
                            endpoint = %endpoint,
                            "VTube Studio left a request unanswered; dropping the session"
                        );
                        close_reason = REASON_UNRESPONSIVE;
                        break;
                    }
                }
            }
        }
        pending.clear();
        clear_session_state(&health_state, &health_tx);

        if let Ok(mut g) = connected_at.write() {
            *g = None;
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Cold;
        }
        let retry = auto_reconnect.load(Ordering::Relaxed);
        set_connection_state(
            &state,
            &health_state,
            &*publisher,
            if retry {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Disconnected
            },
        );
        emit_connection_changed(&*publisher, &endpoint, false, Some(close_reason), None);
        if !retry {
            return;
        }
        backoff.reset();
        reconnecting = true;
    }
}

fn announce_stopped(
    state: &AtomicConnectionState,
    health_state: &RwLock<HealthSnapshot>,
    publisher: &dyn EventPublisher,
    auth_state: &RwLock<AuthState>,
    endpoint: &str,
) {
    set_connection_state(
        state,
        health_state,
        publisher,
        ConnectionState::Disconnected,
    );
    if let Ok(mut g) = auth_state.write() {
        *g = AuthState::Cold;
    }
    emit_connection_changed(publisher, endpoint, false, None, None);
}

/// Hand-only tracking changes arrive with an unchanged `faceFound` and must not re-fire the face kinds.
fn is_repeated_face_state(
    env: &crate::events::RawEnvelope,
    last_face_found: &mut Option<bool>,
) -> bool {
    if env.message_type != "TrackingStatusChangedEvent" {
        return false;
    }
    let found = env.data["faceFound"].as_bool().unwrap_or(false);
    let repeated = *last_face_found == Some(found);
    *last_face_found = Some(found);
    repeated
}

fn apply_seed(
    query: &SeedQuery,
    data: &serde_json::Value,
    last_face_found: &mut Option<bool>,
    health_state: &Arc<RwLock<HealthSnapshot>>,
    health_tx: &broadcast::Sender<HealthDelta>,
) {
    if check_response(data).is_err() {
        return;
    }
    match query {
        SeedQuery::CurrentModel => apply_model(
            data["modelLoaded"].as_bool().unwrap_or(false),
            data["modelName"].as_str().unwrap_or(""),
            data["numberOfLive2DParameters"].as_u64(),
            health_state,
            health_tx,
        ),
        SeedQuery::FaceFound if last_face_found.is_none() => {
            let found = data["found"].as_bool().unwrap_or(false);
            *last_face_found = Some(found);
            apply_tracking(found, health_state, health_tx);
        }
        SeedQuery::FaceFound => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, RwLock};

    use serde_json::json;

    use forge_events::{Event, EventPublisher};
    use forge_platform_core::{
        AtomicConnectionState, CONNECTION_STATE_CHANGED_KIND, ConnectionState,
    };

    use super::{
        connection_state_transition, diff_and_emit_expressions, emit_connection_changed,
        set_connection_state,
    };
    use crate::client::VTUBE_PLATFORM_ID;
    use crate::client::tests::MockPublisher;
    use crate::health::HealthSnapshot;

    const EVERY_CONNECTION_STATE: [ConnectionState; 4] = [
        ConnectionState::Disconnected,
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Reconnecting,
    ];

    struct StateAtAnnouncement {
        state: Arc<AtomicConnectionState>,
        observed: Mutex<Vec<ConnectionState>>,
    }

    impl EventPublisher for StateAtAnnouncement {
        fn publish(&self, event: Event) {
            if event.kind == CONNECTION_STATE_CHANGED_KIND {
                self.observed.lock().unwrap().push(self.state.load());
            }
        }
    }

    #[test]
    fn a_state_transition_announces_the_new_state_only_when_it_actually_changed() {
        for previous in EVERY_CONNECTION_STATE {
            for next in EVERY_CONNECTION_STATE {
                match connection_state_transition(previous, next) {
                    None => assert_eq!(
                        previous, next,
                        "{previous:?} -> {next:?} never reached the bus"
                    ),
                    Some(event) => {
                        assert_ne!(
                            previous, next,
                            "{previous:?} -> {next:?} re-announced a state that did not change"
                        );
                        assert_eq!(event.kind, CONNECTION_STATE_CHANGED_KIND);
                        assert_eq!(
                            event.payload["platform_id"].as_str(),
                            Some(VTUBE_PLATFORM_ID),
                            "{previous:?} -> {next:?} was announced for another platform"
                        );
                        assert_eq!(
                            event.payload["state"],
                            serde_json::to_value(next).unwrap(),
                            "{previous:?} -> {next:?} announced the wrong state"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_shared_announcement_finds_the_new_state_already_stored() {
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected));
        let health_state = RwLock::new(HealthSnapshot::default());
        let publisher = Arc::new(StateAtAnnouncement {
            state: Arc::clone(&state),
            observed: Mutex::new(Vec::new()),
        });
        let walk = [
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Reconnecting,
            ConnectionState::Disconnected,
        ];

        for next in walk {
            set_connection_state(&state, &health_state, &*publisher, next);
        }

        assert_eq!(publisher.observed.lock().unwrap().as_slice(), walk);
    }

    #[test]
    fn the_dialing_flag_tracks_exactly_the_states_that_are_still_reaching_for_a_socket() {
        for (next, expected) in [
            (ConnectionState::Connecting, true),
            (ConnectionState::Reconnecting, true),
            (ConnectionState::Connected, false),
            (ConnectionState::Disconnected, false),
        ] {
            let state = Arc::new(AtomicConnectionState::new(ConnectionState::Connected));
            let health_state = RwLock::new(HealthSnapshot::default());

            set_connection_state(
                &state,
                &health_state,
                &*MockPublisher::new().publisher(),
                next,
            );

            assert_eq!(
                health_state.read().unwrap().dialing,
                expected,
                "dialing after moving to {next:?}"
            );
        }
    }

    #[test]
    fn diff_emits_state_changed_only_when_active_value_flips() {
        for (prev, curr, expected) in [
            (true, false, Some(false)),
            (false, true, Some(true)),
            (true, true, None),
            (false, false, None),
        ] {
            let publisher = MockPublisher::new();
            let mut baseline = Some(HashMap::from([("smile.exp3.json".to_owned(), prev)]));
            let data = json!({ "expressions": [{ "file": "smile.exp3.json", "active": curr }] });

            diff_and_emit_expressions(&data, &mut baseline, &*publisher.publisher());

            let events = publisher.events.lock().unwrap();
            match expected {
                Some(is_active) => {
                    assert_eq!(events.len(), 1, "flip {prev} to {curr} must emit once");
                    assert_eq!(events[0].kind, "vtube.expression.state_changed");
                    assert_eq!(events[0].payload["expression_file"], "smile.exp3.json");
                    assert_eq!(events[0].payload["is_active"], is_active);
                }
                None => assert!(
                    events.is_empty(),
                    "unchanged {prev} to {curr} must not emit"
                ),
            }
        }
    }

    #[test]
    fn first_snapshot_seeds_baseline_without_emitting() {
        let publisher = MockPublisher::new();
        let mut baseline: Option<HashMap<String, bool>> = None;
        let data = json!({ "expressions": [
            { "file": "a.exp3.json", "active": true },
            { "file": "b.exp3.json", "active": false },
        ]});

        diff_and_emit_expressions(&data, &mut baseline, &*publisher.publisher());

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "first sighting must seed silently"
        );
        let seeded = baseline.unwrap();
        assert_eq!(seeded.get("a.exp3.json"), Some(&true));
        assert_eq!(seeded.get("b.exp3.json"), Some(&false));
    }

    #[test]
    fn newly_appearing_file_seeds_silently_then_later_flip_emits() {
        let publisher = MockPublisher::new();
        let mut baseline = Some(HashMap::from([("known.exp3.json".to_owned(), true)]));

        let first = json!({ "expressions": [
            { "file": "known.exp3.json", "active": true },
            { "file": "fresh.exp3.json", "active": true },
        ]});
        diff_and_emit_expressions(&first, &mut baseline, &*publisher.publisher());
        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "a file absent from the previous snapshot must seed silently"
        );

        let second = json!({ "expressions": [
            { "file": "known.exp3.json", "active": true },
            { "file": "fresh.exp3.json", "active": false },
        ]});
        diff_and_emit_expressions(&second, &mut baseline, &*publisher.publisher());

        let events = publisher.events.lock().unwrap();
        assert_eq!(
            events.len(),
            1,
            "the newly-known file's flip must emit once"
        );
        assert_eq!(events[0].payload["expression_file"], "fresh.exp3.json");
        assert_eq!(events[0].payload["is_active"], false);
    }

    #[test]
    fn baseline_reset_reseeds_next_snapshot_silently_even_for_known_files() {
        let publisher = MockPublisher::new();
        let mut baseline: Option<HashMap<String, bool>> = None;

        let seed = json!({ "expressions": [{ "file": "wave.exp3.json", "active": true }] });
        diff_and_emit_expressions(&seed, &mut baseline, &*publisher.publisher());

        baseline = None;

        let after_switch =
            json!({ "expressions": [{ "file": "wave.exp3.json", "active": false }] });
        diff_and_emit_expressions(&after_switch, &mut baseline, &*publisher.publisher());

        assert!(
            publisher.events.lock().unwrap().is_empty(),
            "after a reset the flipped known file must reseed silently, not emit"
        );
        assert_eq!(baseline.unwrap().get("wave.exp3.json"), Some(&false));
    }

    #[test]
    fn malformed_expression_items_are_skipped_without_panicking() {
        for data in [
            json!({}),
            json!({ "expressions": "not-an-array" }),
            json!({ "expressions": [{ "active": true }] }),
            json!({ "expressions": [{ "file": "x.exp3.json" }] }),
            json!({ "expressions": [{ "file": 42, "active": "yes" }] }),
            json!({ "expressions": [null, 7, "str"] }),
        ] {
            let publisher = MockPublisher::new();
            let mut baseline: Option<HashMap<String, bool>> = None;

            diff_and_emit_expressions(&data, &mut baseline, &*publisher.publisher());

            assert!(
                baseline.is_some(),
                "helper must run to completion and seed even on malformed input: {data}"
            );
            assert!(
                publisher.events.lock().unwrap().is_empty(),
                "malformed first snapshot must not emit: {data}"
            );
        }
    }

    #[test]
    fn connection_changed_uses_is_connected_key_not_legacy_connected() {
        let publisher = MockPublisher::new();
        emit_connection_changed(&*publisher.publisher(), "ws://x:1", true, None, None);

        let events = publisher.events.lock().unwrap();
        let ev = events.first().unwrap();
        assert_eq!(ev.payload["is_connected"], true);
        assert!(
            ev.payload.get("connected").is_none(),
            "legacy 'connected' key must be gone after the is_ rename"
        );
    }

    #[test]
    fn connection_changed_detail_is_null_when_absent_and_string_when_present() {
        let publisher = MockPublisher::new();
        let p = publisher.publisher();
        emit_connection_changed(&*p, "ws://x:1", false, Some("auth_required"), None);
        emit_connection_changed(
            &*p,
            "ws://x:1",
            false,
            Some("connect_failed"),
            Some("dns failure".to_owned()),
        );

        let events = publisher.events.lock().unwrap();
        assert!(
            events[0].payload["detail"].is_null(),
            "absent detail must serialize as JSON null, not an empty string"
        );
        assert_eq!(events[0].payload["reason"], "auth_required");
        assert_eq!(events[1].payload["detail"], "dns failure");
    }

    mod session {
        use std::sync::Arc;
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        use forge_platform_core::{BuiltinControl, BuiltinHealth, HealthValue};
        use serde_json::json;

        use super::super::HANDSHAKE_REPLY_TIMEOUT;
        use crate::client::VTubeClient;
        use crate::client::tests::{
            FakeVts, MockCreds, MockPublisher, PeerConn, freeze_clock, stored_token_creds,
            wait_paused,
        };
        use crate::sink::VTubeSink;

        const SETTLE: Duration = Duration::from_secs(5);
        const LONG_WAIT: Duration = Duration::from_secs(60);
        const PROMPT: Duration = Duration::from_secs(1);

        async fn connected_session(
            vts: &mut FakeVts,
            publisher: &Arc<MockPublisher>,
        ) -> (VTubeClient, PeerConn) {
            let client = vts.connect(publisher, &stored_token_creds());
            let conn = vts.logged_in_conn().await;
            assert!(
                wait_paused(SETTLE, || client.connection_state().is_connected()).await,
                "the session never reached connected"
            );
            (client, conn)
        }

        fn face_kinds(publisher: &MockPublisher) -> Vec<String> {
            publisher
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.kind.starts_with("vtube.tracking.face_"))
                .map(|e| e.kind.clone())
                .collect()
        }

        fn hotkey_event_seen(publisher: &MockPublisher, hotkey_id: &str) -> bool {
            publisher
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|e| e.kind == "vtube.hotkey.triggered" && e.payload["hotkey_id"] == hotkey_id)
        }

        fn metric(client: &VTubeClient, label: &str) -> HealthValue {
            client
                .metrics()
                .into_iter()
                .find(|m| m.label == label)
                .unwrap()
                .value
        }

        fn is_expression_poll(frame: &serde_json::Value) -> bool {
            frame["messageType"] == "ExpressionStateRequest" && frame["data"]["details"] == false
        }

        async fn answer_seeds(conn: &mut PeerConn) {
            let model = conn.expect("CurrentModelRequest", SETTLE).await;
            conn.tx.reply(
                &model,
                json!({
                    "modelLoaded": true,
                    "modelName": "MyAvatar",
                    "modelID": "m-1",
                    "numberOfLive2DParameters": 42
                }),
            );
            let face = conn.expect("FaceFoundRequest", SETTLE).await;
            conn.tx.reply(&face, json!({ "found": true }));
        }

        #[tokio::test(start_paused = true)]
        async fn a_request_left_unanswered_drops_the_session_as_unresponsive_and_redials() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, conn) = connected_session(&mut vts, &publisher).await;
            let _peer = conn.answer_all_except(|f| f["messageType"] == "HotkeyTriggerRequest");

            let _ = client.trigger_hotkey("hk-1").await;

            assert!(
                wait_paused(SETTLE, || publisher
                    .disconnected_with_reason("unresponsive"))
                .await,
                "a request past its deadline must drop the session as unresponsive"
            );
            assert!(
                vts.next_conn(LONG_WAIT).await.is_some(),
                "an unresponsive session must be redialed"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn a_peer_silent_after_the_upgrade_fails_the_handshake_at_the_reply_deadline() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let frozen = freeze_clock();
            let _client = vts.connect(&publisher, &stored_token_creds());
            let mut conn = vts.next_conn(SETTLE).await.unwrap();
            conn.expect("AuthenticationRequest", SETTLE).await;
            let asked_at = tokio::time::Instant::now();
            drop(frozen);

            assert!(
                wait_paused(LONG_WAIT, || publisher
                    .disconnected_with_reason("auth_failed"))
                .await,
                "a login nobody answers must end as auth_failed"
            );
            assert!(
                asked_at.elapsed() >= HANDSHAKE_REPLY_TIMEOUT,
                "auth_failed arrived after {:?}, before the handshake deadline",
                asked_at.elapsed()
            );
            drop(conn);
        }

        #[tokio::test(start_paused = true)]
        async fn disconnecting_during_a_silent_handshake_returns_promptly() {
            for (phase, creds, request) in [
                ("login", stored_token_creds(), "AuthenticationRequest"),
                (
                    "approval popup",
                    MockCreds::new(),
                    "AuthenticationTokenRequest",
                ),
            ] {
                let mut vts = FakeVts::bind().await;
                let client = vts.connect(&MockPublisher::new(), &creds);
                let mut conn = vts.next_conn(SETTLE).await.unwrap();
                conn.expect(request, SETTLE).await;

                let outcome = tokio::time::timeout(PROMPT, client.disconnect()).await;

                assert!(
                    outcome.is_ok(),
                    "disconnect during the {phase} waited for the peer instead of cancelling"
                );
                drop(conn);
            }
        }

        #[tokio::test(start_paused = true)]
        async fn two_unanswered_expression_polls_drop_the_session_even_while_events_flow() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (_client, conn) = connected_session(&mut vts, &publisher).await;
            let (peer, unanswered_polls) = conn.answer_all_except(is_expression_poll);
            let events = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    peer.event(
                        "HotkeyTriggeredEvent",
                        json!({ "hotkeyID": "hk-1", "hotkeyName": "Wave" }),
                    );
                }
            });

            let dropped = wait_paused(LONG_WAIT, || {
                publisher.disconnected_with_reason("unresponsive")
            })
            .await;
            events.abort();

            assert!(dropped, "a peer that stops answering polls must be dropped");
            assert_eq!(
                unanswered_polls.load(Ordering::SeqCst),
                2,
                "the session must drop on the second unanswered poll"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn a_peer_that_answers_every_poll_stays_connected() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, conn) = connected_session(&mut vts, &publisher).await;
            let _peer = conn.answer_all_except(|_| false);

            let dropped = wait_paused(Duration::from_secs(30), || {
                publisher.disconnected_event().is_some()
            })
            .await;

            assert!(!dropped, "a responsive peer was dropped");
            assert!(client.connection_state().is_connected());
        }

        #[tokio::test(start_paused = true)]
        async fn the_current_model_and_face_replies_seed_the_model_and_tracking_tiles() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, mut conn) = connected_session(&mut vts, &publisher).await;
            let mut deltas = client.health_tx.subscribe();

            answer_seeds(&mut conn).await;

            assert!(
                wait_paused(SETTLE, || matches!(
                    metric(&client, "TRACKING"),
                    HealthValue::Status { active: true, .. }
                ))
                .await,
                "the face seed never reached the TRACKING tile"
            );
            let model_index = client
                .metrics()
                .iter()
                .position(|m| m.label == "MODEL")
                .unwrap();
            let model_delta = std::iter::from_fn(|| deltas.try_recv().ok())
                .find(|d| usize::from(d.index) == model_index)
                .map(|d| d.new_value);
            assert_eq!(
                model_delta,
                Some(HealthValue::Text {
                    primary: "MyAvatar".to_owned(),
                    secondary: Some("42 parameters".to_owned()),
                })
            );
        }

        #[tokio::test(start_paused = true)]
        async fn a_session_end_clears_the_model_and_tracking_tiles() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, mut conn) = connected_session(&mut vts, &publisher).await;
            answer_seeds(&mut conn).await;
            assert!(
                wait_paused(SETTLE, || matches!(
                    metric(&client, "TRACKING"),
                    HealthValue::Status { active: true, .. }
                ))
                .await,
                "precondition: the seeds must land first"
            );

            conn.tx.close();

            assert!(
                wait_paused(SETTLE, || publisher
                    .disconnected_with_reason("socket_closed"))
                .await,
                "precondition: the session must end"
            );
            assert_eq!(
                (metric(&client, "MODEL"), metric(&client, "TRACKING")),
                (
                    HealthValue::Text {
                        primary: "\u{2014}".to_owned(),
                        secondary: Some("not loaded".to_owned()),
                    },
                    HealthValue::Status {
                        label: "Off".to_owned(),
                        active: false,
                        detail: None,
                    },
                )
            );
        }

        #[tokio::test(start_paused = true)]
        async fn face_triggers_fire_only_when_the_face_state_changes() {
            type FaceCase = (
                &'static str,
                Option<bool>,
                &'static [(bool, bool)],
                &'static [&'static str],
            );
            let cases: [FaceCase; 2] = [
                (
                    "hand changes without a seed",
                    None,
                    &[(true, false), (true, true), (false, true), (false, false)],
                    &["vtube.tracking.face_found", "vtube.tracking.face_lost"],
                ),
                (
                    "hand change after the face seed",
                    Some(true),
                    &[(true, true)],
                    &[],
                ),
            ];
            for (case, seed, changes, expected) in cases {
                let mut vts = FakeVts::bind().await;
                let publisher = MockPublisher::new();
                let (_client, mut conn) = connected_session(&mut vts, &publisher).await;
                if let Some(found) = seed {
                    let face = conn.expect("FaceFoundRequest", SETTLE).await;
                    conn.tx.reply(&face, json!({ "found": found }));
                }

                for &(face_found, left_hand_found) in changes {
                    conn.tx.event(
                        "TrackingStatusChangedEvent",
                        json!({
                            "faceFound": face_found,
                            "leftHandFound": left_hand_found,
                            "rightHandFound": false
                        }),
                    );
                }
                conn.tx.event(
                    "HotkeyTriggeredEvent",
                    json!({ "hotkeyID": "sentinel", "hotkeyName": "sentinel" }),
                );
                assert!(
                    wait_paused(SETTLE, || hotkey_event_seen(&publisher, "sentinel")).await,
                    "{case}: the events were never processed"
                );

                assert_eq!(face_kinds(&publisher), expected, "{case}");
            }
        }
    }
}
