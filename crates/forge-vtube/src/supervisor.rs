use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast, mpsc, oneshot, watch};
use tokio::time::Instant;

use forge_events::{Event, EventPublisher, EventSource, TypedEvent};
use forge_net::{Backoff, DEFAULT_BACKOFF_BASE};
use forge_platform_core::{
    AtomicConnectionState, ConnectionState, ConnectionTransition, HealthDelta,
    connection_state_changed_event,
};
use forge_storage::CredentialsRepo;
use forge_types::MessageRef;

use crate::auth::AuthState;
use crate::client::VTUBE_PLATFORM_ID;
use crate::error::VTubeError;
use crate::health::{
    HealthSnapshot, apply_model, apply_tracking, clear_session_state, update_from_event,
};
use crate::timing::VTubeTiming;
use crate::vocabulary::fields::expression as expression_fields;
use crate::vocabulary::{VTubeConnectionChanged, VTubeDisconnectReason, event_kinds, messages};
use crate::vts::endpoint::{is_secure_endpoint, split_endpoint};
use crate::vts::event::{VtsEvent, subscribe_all};
use crate::vts::message_type::{
    AUTHENTICATION_REQUEST, AUTHENTICATION_RESPONSE, AUTHENTICATION_TOKEN_REQUEST,
    AUTHENTICATION_TOKEN_RESPONSE, CURRENT_MODEL_REQUEST, EXPRESSION_STATE_REQUEST,
    FACE_FOUND_REQUEST,
};
use crate::vts::protocol::{
    API_ERROR_MESSAGE_TYPE, TOKEN_REQUEST_DENIED_ERROR_ID, check_response, is_reply, new_request,
};
use crate::vts::request::PendingRequest;
use crate::vts::socket::{
    Inbound, TransportLayer, VtsWs, dial_within, exchange, next_inbound, send_text,
    send_ws_msg_within,
};

pub(crate) const RECONNECT_BASE_DELAY: Duration = DEFAULT_BACKOFF_BASE;
pub(crate) const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const EXPRESSION_POLL_INTERVAL: Duration = Duration::from_secs(3);
pub(crate) const EXPRESSION_POLL_TIMEOUT: Duration = Duration::from_secs(5);
const UNANSWERED_POLLS_BEFORE_SUSPECT: u32 = 2;
pub(crate) const REQUEST_SWEEP_INTERVAL: Duration = Duration::from_secs(1);
pub(crate) const TOKEN_APPROVAL_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const HANDSHAKE_REPLY_TIMEOUT: Duration = Duration::from_secs(10);

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
    reason: Option<VTubeDisconnectReason>,
    detail: Option<String>,
) {
    VTubeConnectionChanged {
        is_connected,
        endpoint: endpoint.to_owned(),
        reason,
        detail,
    }
    .publish(publisher, EventSource::VTube, None);
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
) -> bool {
    let current = snapshot_expressions(data);
    let changed = baseline.as_ref() != Some(&current);
    if let Some(prev) = baseline {
        for (file, is_active) in &current {
            if prev
                .get(file)
                .is_some_and(|prev_active| prev_active != is_active)
            {
                publisher.publish(Event::new(
                    EventSource::VTube,
                    event_kinds::EXPRESSION_STATE_CHANGED,
                    serde_json::json!({
                        (expression_fields::EXPRESSION_FILE): file,
                        (expression_fields::IS_ACTIVE): is_active,
                    }),
                ));
            }
        }
    }
    *baseline = Some(current);
    changed
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

async fn request_new_token(
    ws: &mut VtsWs,
    endpoint: &str,
    approval_timeout: Duration,
) -> Result<String, VTubeError> {
    tracing::debug!(
        endpoint,
        "sending AuthenticationTokenRequest, awaiting popup"
    );
    let msg = exchange(
        ws,
        AUTHENTICATION_TOKEN_REQUEST,
        serde_json::json!({
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME
        }),
        approval_timeout,
    )
    .await
    .map_err(|e| match e {
        VTubeError::Timeout => VTubeError::TokenTimeout,
        other => other,
    })?;

    match msg["messageType"].as_str().unwrap_or("") {
        AUTHENTICATION_TOKEN_RESPONSE => msg["data"]["authenticationToken"]
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
        _ => Err(unexpected_reply(AUTHENTICATION_TOKEN_RESPONSE, &msg)),
    }
}

async fn authenticate_with_token(
    ws: &mut VtsWs,
    creds: &dyn CredentialsRepo,
    token: &str,
    endpoint: &str,
    reply_timeout: Duration,
) -> Result<(), VTubeError> {
    tracing::debug!(endpoint, "sending AuthenticationRequest");
    let msg = exchange(
        ws,
        AUTHENTICATION_REQUEST,
        serde_json::json!({
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME,
            "authenticationToken": token
        }),
        reply_timeout,
    )
    .await?;

    let rejection = match msg["messageType"].as_str().unwrap_or("") {
        AUTHENTICATION_RESPONSE if msg["data"]["authenticated"].as_bool() == Some(true) => {
            return Ok(());
        }
        AUTHENTICATION_RESPONSE => msg["data"]["reason"].as_str().unwrap_or("").to_owned(),
        API_ERROR_MESSAGE_TYPE if api_error_id(&msg) == Some(TOKEN_REQUEST_DENIED_ERROR_ID) => {
            msg["data"]["message"].as_str().unwrap_or("").to_owned()
        }
        API_ERROR_MESSAGE_TYPE => return Err(api_error(&msg)),
        _ => return Err(unexpected_reply(AUTHENTICATION_RESPONSE, &msg)),
    };

    if let Err(error) = crate::credentials::clear(creds).await {
        tracing::warn!(%error, "rejected vtube token could not be cleared");
    }
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
    timing: VTubeTiming,
) -> Result<(), VTubeError> {
    let stored = crate::credentials::load(creds).await?;

    let token = if let Some(c) = stored {
        tracing::debug!(endpoint, "using stored VTube Studio credential");
        c.token.expose().to_owned()
    } else {
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::AwaitingApproval;
        }
        emit_connection_changed(
            publisher,
            endpoint,
            false,
            Some(VTubeDisconnectReason::AwaitingApproval),
            None,
        );
        request_new_token(ws, endpoint, timing.token_approval_timeout).await?
    };

    authenticate_with_token(ws, creds, &token, endpoint, timing.handshake_reply_timeout).await?;

    let (host, port) = split_endpoint(endpoint);
    let secure = is_secure_endpoint(endpoint);
    if let Err(e) = crate::credentials::store(creds, &token, &host, port, secure).await {
        tracing::warn!(endpoint, error = %e, "failed to persist VTube Studio credential");
    }
    Ok(())
}

fn connection_state_transition(
    previous: ConnectionState,
    next: ConnectionState,
    transition: ConnectionTransition,
) -> Option<Event> {
    (previous != next).then(|| connection_state_changed_event(VTUBE_PLATFORM_ID, next, transition))
}

pub(crate) fn set_connection_state(
    state: &AtomicConnectionState,
    publisher: &dyn EventPublisher,
    new_state: ConnectionState,
    transition: ConnectionTransition,
) {
    let previous = state.load();
    state.store(new_state);
    if let Some(event) = connection_state_transition(previous, new_state, transition) {
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
    pub(crate) catalog_signal: crate::catalog::CatalogSignal,
    pub(crate) connected_notifier: mpsc::UnboundedSender<()>,
    pub(crate) auto_reconnect: watch::Receiver<bool>,
    pub(crate) last_failure: Arc<RwLock<Option<MessageRef>>>,
    pub(crate) timing: VTubeTiming,
    pub(crate) transport_layer: TransportLayer,
}

enum RetryWait {
    Elapsed,
    Disabled,
    Shutdown,
}

async fn wait_before_retry(
    delay: Duration,
    shutdown: &Notify,
    auto_reconnect: &mut watch::Receiver<bool>,
) -> RetryWait {
    tokio::select! {
        () = tokio::time::sleep(delay) => RetryWait::Elapsed,
        () = shutdown.notified() => RetryWait::Shutdown,
        _ = auto_reconnect.wait_for(|enabled| !*enabled) => RetryWait::Disabled,
    }
}

async fn park_until_auto_reconnect(
    shutdown: &Notify,
    auto_reconnect: &mut watch::Receiver<bool>,
) -> bool {
    tokio::select! {
        () = shutdown.notified() => false,
        resumed = auto_reconnect.wait_for(|enabled| *enabled) => resumed.is_ok(),
    }
}

pub(crate) fn record_failure(slot: &RwLock<Option<MessageRef>>, failure: Option<MessageRef>) {
    if let Ok(mut g) = slot.write() {
        *g = failure;
    }
}

fn record_reason(slot: &RwLock<Option<MessageRef>>, reason: VTubeDisconnectReason) {
    record_failure(slot, messages::for_disconnect_reason(reason));
}

fn settle_unless_retrying(
    retry: bool,
    state: &AtomicConnectionState,
    publisher: &dyn EventPublisher,
) {
    if retry {
        return;
    }
    set_connection_state(
        state,
        publisher,
        ConnectionState::Disconnected,
        ConnectionTransition::Unexpected,
    );
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
        catalog_signal,
        connected_notifier,
        mut auto_reconnect,
        last_failure,
        timing,
        transport_layer,
    } = ctx;

    let mut backoff = Backoff::new(timing.reconnect_base_delay, timing.reconnect_max_delay);
    let mut reconnecting = false;

    loop {
        if reconnecting {
            let delay = backoff.next_delay();
            let wait = if *auto_reconnect.borrow_and_update() {
                tracing::info!(
                    endpoint = %endpoint,
                    delay_ms = delay.as_millis(),
                    "reconnecting to VTube Studio"
                );
                wait_before_retry(delay, &shutdown, &mut auto_reconnect).await
            } else {
                RetryWait::Disabled
            };
            match wait {
                RetryWait::Elapsed => {}
                RetryWait::Shutdown => {
                    set_connection_state(
                        &state,
                        &*publisher,
                        ConnectionState::Disconnected,
                        ConnectionTransition::Expected,
                    );
                    emit_connection_changed(&*publisher, &endpoint, false, None, None);
                    return;
                }
                RetryWait::Disabled => {
                    tracing::info!(
                        endpoint = %endpoint,
                        "VTube Studio auto-reconnect is off; waiting for a manual connect"
                    );
                    if state.load() != ConnectionState::Disconnected {
                        set_connection_state(
                            &state,
                            &*publisher,
                            ConnectionState::Disconnected,
                            ConnectionTransition::Expected,
                        );
                        emit_connection_changed(&*publisher, &endpoint, false, None, None);
                    }
                    if !park_until_auto_reconnect(&shutdown, &mut auto_reconnect).await {
                        return;
                    }
                    backoff.reset();
                }
            }
        }

        set_connection_state(
            &state,
            &*publisher,
            if reconnecting {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Connecting
            },
            ConnectionTransition::Unexpected,
        );
        tracing::debug!(endpoint = %endpoint, "attempting VTube Studio connection");

        let connect_attempt = tokio::select! {
            outcome = dial_within(&endpoint, timing.connect_timeout) => outcome,
            () = shutdown.notified() => {
                set_connection_state(&state, &*publisher, ConnectionState::Disconnected, ConnectionTransition::Expected);
                emit_connection_changed(&*publisher, &endpoint, false, None, None);
                return;
            }
        };

        let mut ws = match connect_attempt {
            Ok(ws) => transport_layer(ws),
            Err(failed) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %failed.detail,
                    "VTube Studio connection attempt failed"
                );
                record_failure(&last_failure, Some(failed.message));
                settle_unless_retrying(*auto_reconnect.borrow_and_update(), &state, &*publisher);
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::ConnectFailed),
                    Some(failed.detail),
                );
                reconnecting = true;
                continue;
            }
        };

        let auth_outcome = tokio::select! {
            outcome = run_auth(&mut ws, &*creds, &endpoint, &auth_state, &*publisher, timing) => outcome,
            () = shutdown.notified() => {
                announce_stopped(&state, &*publisher, &auth_state, &endpoint);
                return;
            }
        };
        match auth_outcome {
            Ok(()) => {}
            Err(VTubeError::TokenRejected) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                record_reason(&last_failure, VTubeDisconnectReason::AuthRequired);
                set_connection_state(
                    &state,
                    &*publisher,
                    ConnectionState::Disconnected,
                    ConnectionTransition::Unexpected,
                );
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::AuthRequired),
                    None,
                );
                return;
            }
            Err(VTubeError::TokenDenied) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                record_reason(&last_failure, VTubeDisconnectReason::AuthDenied);
                set_connection_state(
                    &state,
                    &*publisher,
                    ConnectionState::Disconnected,
                    ConnectionTransition::Unexpected,
                );
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::AuthDenied),
                    None,
                );
                return;
            }
            Err(VTubeError::TokenTimeout) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                record_reason(&last_failure, VTubeDisconnectReason::AuthTimeout);
                set_connection_state(
                    &state,
                    &*publisher,
                    ConnectionState::Disconnected,
                    ConnectionTransition::Unexpected,
                );
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::AuthTimeout),
                    None,
                );
                return;
            }
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "auth failed, will retry"
                );
                let detail = e.to_string();
                record_reason(&last_failure, VTubeDisconnectReason::AuthFailed);
                settle_unless_retrying(*auto_reconnect.borrow_and_update(), &state, &*publisher);
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::AuthFailed),
                    Some(detail),
                );
                reconnecting = true;
                continue;
            }
        }

        let subscribe_outcome = tokio::select! {
            outcome = tokio::time::timeout(timing.handshake_reply_timeout, subscribe_all(&mut ws)) => {
                outcome.unwrap_or(Err(VTubeError::Timeout))
            }
            () = shutdown.notified() => {
                announce_stopped(&state, &*publisher, &auth_state, &endpoint);
                return;
            }
        };
        let mut subscriptions = match subscribe_outcome {
            Ok(requested) => requested,
            Err(e) => {
                tracing::debug!(endpoint = %endpoint, error = %e, "event subscription failed, will retry");
                let detail = e.to_string();
                record_reason(&last_failure, VTubeDisconnectReason::SubscribeFailed);
                settle_unless_retrying(*auto_reconnect.borrow_and_update(), &state, &*publisher);
                emit_connection_changed(
                    &*publisher,
                    &endpoint,
                    false,
                    Some(VTubeDisconnectReason::SubscribeFailed),
                    Some(detail),
                );
                reconnecting = true;
                continue;
            }
        };

        if let Ok(mut g) = connected_at.write() {
            *g = Some(OffsetDateTime::now_utc());
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Connected;
        }
        record_failure(&last_failure, None);
        set_connection_state(
            &state,
            &*publisher,
            ConnectionState::Connected,
            ConnectionTransition::Unexpected,
        );
        emit_connection_changed(&*publisher, &endpoint, true, None, None);
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the connect waiter may already have stopped listening"
        )]
        let _ = connected_notifier.send(());
        catalog_signal.bump_options();
        tracing::info!(endpoint = %endpoint, "connected and authenticated to VTube Studio");

        let mut pending: HashMap<String, InFlight> = HashMap::new();
        let mut last_face_found: Option<bool> = None;
        let mut expr_tick = tokio::time::interval(timing.expression_poll_interval);
        let mut sweep_tick = tokio::time::interval(timing.request_sweep_interval);
        let mut expr_baseline: Option<HashMap<String, bool>> = None;
        let mut expr_poll: Option<ExpressionPoll> = None;
        let mut unanswered_polls: u32 = 0;
        let mut close_reason = VTubeDisconnectReason::SocketClosed;

        let mut seeds = tokio::select! {
            sent = send_seed_queries(&mut ws, timing.request_timeout) => sent,
            () = shutdown.notified() => {
                announce_stopped(&state, &*publisher, &auth_state, &endpoint);
                return;
            }
        };
        content_notifier.notify_model_changed();
        content_notifier.notify_items_changed();

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    clear_session_state(&health_state, &health_tx);
                    catalog_signal.bump_options();
                    announce_stopped(&state, &*publisher, &auth_state, &endpoint);
                    return;
                }
                Some(req) = req_rx.recv() => {
                    match tokio::time::timeout(timing.request_timeout, send_text(&mut ws, req.payload)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::info!(endpoint = %endpoint, %error, "VTube Studio write failed; dropping the session");
                            break;
                        }
                        Err(_) => {
                            tracing::info!(endpoint = %endpoint, "VTube Studio write stalled; dropping the session");
                            break;
                        }
                    }
                    pending.insert(
                        req.request_id,
                        InFlight {
                            respond_to: req.respond_to,
                            sent_at: Instant::now(),
                        },
                    );
                }
                inbound = next_inbound(&mut ws) => {
                    let val = match inbound {
                        Inbound::Closed => {
                            tracing::info!(endpoint = %endpoint, "VTube Studio connection closed");
                            break;
                        }
                        Inbound::Skipped => continue,
                        Inbound::Envelope(val) => val,
                    };
                    let msg_type = val["messageType"].as_str().unwrap_or("");
                    if is_reply(msg_type) {
                        let req_id = val["requestID"].as_str().unwrap_or("");
                        if let Some(in_flight) = pending.remove(req_id) {
                            #[expect(
                                clippy::let_underscore_must_use,
                                reason = "the requester may stop waiting for the reply"
                            )]
                            let _ = in_flight.respond_to.send(val["data"].clone());
                        } else if let Some(query) = seeds.remove(req_id) {
                            apply_seed(
                                &query,
                                &val["data"],
                                &mut last_face_found,
                                &health_state,
                                &health_tx,
                            );
                        } else if let Some(event_name) = subscriptions.remove(req_id) {
                            if msg_type == API_ERROR_MESSAGE_TYPE {
                                tracing::warn!(
                                    endpoint = %endpoint,
                                    event_name,
                                    error_id = ?api_error_id(&val),
                                    "VTube Studio rejected an event subscription; its triggers will not fire"
                                );
                            }
                        } else if msg_type == API_ERROR_MESSAGE_TYPE {
                            tracing::debug!(
                                endpoint = %endpoint,
                                error_id = ?api_error_id(&val),
                                "VTube Studio error for a request nobody awaits"
                            );
                        }
                    } else if let Some(event) = VtsEvent::decode(&val) {
                        if matches!(event, VtsEvent::ModelLoaded { .. }) {
                            content_notifier.notify_model_changed();
                            expr_baseline = None;
                        }
                        if matches!(event, VtsEvent::ModelConfigChanged { .. }) {
                            content_notifier.notify_model_changed();
                        }
                        if event.changes_scene_items() {
                            catalog_signal.bump_options();
                            content_notifier.notify_items_changed();
                        }
                        if !is_repeated_face_state(&event, &mut last_face_found) {
                            crate::events::dispatch_vts_event(&event, &*publisher);
                        }
                        update_from_event(&event, &health_state, &health_tx);
                    }
                }
                _ = expr_tick.tick(), if expr_poll.is_none() => {
                    let req = new_request(EXPRESSION_STATE_REQUEST, serde_json::json!({ "details": false }));
                    if let Err(error) = send_ws_msg_within(&mut ws, &req, timing.request_timeout).await {
                        tracing::info!(endpoint = %endpoint, %error, "VTube Studio poll write failed; dropping the session");
                        break;
                    }
                    let (tx, rx) = oneshot::channel();
                    let sent_at = Instant::now();
                    pending.insert(
                        req.request_id.clone(),
                        InFlight { respond_to: tx, sent_at },
                    );
                    expr_poll = Some(ExpressionPoll {
                        request_id: req.request_id,
                        reply: rx,
                        deadline: sent_at + timing.expression_poll_timeout,
                    });
                }
                result = await_expression_poll(&mut expr_poll) => {
                    let finished = expr_poll.take();
                    if let Some(data) = result {
                        unanswered_polls = 0;
                        if check_response(&data).is_ok()
                            && diff_and_emit_expressions(&data, &mut expr_baseline, &*publisher)
                        {
                            content_notifier.notify_items_changed();
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
                            close_reason = VTubeDisconnectReason::Unresponsive;
                            break;
                        }
                    }
                }
                _ = sweep_tick.tick() => {
                    if pending.values().any(|f| f.sent_at.elapsed() >= timing.unresponsive_threshold()) {
                        tracing::warn!(
                            endpoint = %endpoint,
                            "VTube Studio left a request unanswered; dropping the session"
                        );
                        close_reason = VTubeDisconnectReason::Unresponsive;
                        break;
                    }
                }
            }
        }
        pending.clear();
        clear_session_state(&health_state, &health_tx);
        catalog_signal.bump_options();

        if let Ok(mut g) = connected_at.write() {
            *g = None;
        }
        if let Ok(mut g) = auth_state.write() {
            *g = AuthState::Cold;
        }
        record_reason(&last_failure, close_reason);
        let retry = *auto_reconnect.borrow_and_update();
        settle_unless_retrying(retry, &state, &*publisher);
        if retry {
            set_connection_state(
                &state,
                &*publisher,
                ConnectionState::Reconnecting,
                ConnectionTransition::Unexpected,
            );
        }
        emit_connection_changed(&*publisher, &endpoint, false, Some(close_reason), None);
        backoff.reset();
        reconnecting = true;
    }
}

async fn send_seed_queries(ws: &mut VtsWs, limit: Duration) -> HashMap<String, SeedQuery> {
    let mut seeds = HashMap::new();
    for (msg_type, query) in [
        (CURRENT_MODEL_REQUEST, SeedQuery::CurrentModel),
        (FACE_FOUND_REQUEST, SeedQuery::FaceFound),
    ] {
        let req = new_request(msg_type, serde_json::json!({}));
        if send_ws_msg_within(ws, &req, limit).await.is_err() {
            break;
        }
        seeds.insert(req.request_id, query);
    }
    seeds
}

fn announce_stopped(
    state: &AtomicConnectionState,
    publisher: &dyn EventPublisher,
    auth_state: &RwLock<AuthState>,
    endpoint: &str,
) {
    set_connection_state(
        state,
        publisher,
        ConnectionState::Disconnected,
        ConnectionTransition::Expected,
    );
    if let Ok(mut g) = auth_state.write() {
        *g = AuthState::Cold;
    }
    emit_connection_changed(publisher, endpoint, false, None, None);
}

fn is_repeated_face_state(event: &VtsEvent, last_face_found: &mut Option<bool>) -> bool {
    let VtsEvent::TrackingStatusChanged { face_found, .. } = *event else {
        return false;
    };
    let repeated = *last_face_found == Some(face_found);
    *last_face_found = Some(face_found);
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
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use forge_events::{Event, EventPublisher, EventSource};
    use forge_events::{PlatformConnectionChanged, TypedEvent};
    use forge_platform_core::{AtomicConnectionState, ConnectionState, ConnectionTransition};

    use super::{
        connection_state_transition, diff_and_emit_expressions, emit_connection_changed,
        set_connection_state,
    };
    use crate::client::VTUBE_PLATFORM_ID;
    use crate::client::tests::MockPublisher;
    use crate::vocabulary::VTubeDisconnectReason;

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
            if event.kind == PlatformConnectionChanged::KIND {
                self.observed.lock().unwrap().push(self.state.load());
            }
        }
    }

    #[test]
    fn a_state_transition_announces_the_new_state_only_when_it_actually_changed() {
        for previous in EVERY_CONNECTION_STATE {
            for next in EVERY_CONNECTION_STATE {
                for transition in [
                    ConnectionTransition::Expected,
                    ConnectionTransition::Unexpected,
                ] {
                    match connection_state_transition(previous, next, transition) {
                        None => assert_eq!(
                            previous, next,
                            "{previous:?} -> {next:?} never reached the bus"
                        ),
                        Some(event) => {
                            assert_ne!(
                                previous, next,
                                "{previous:?} -> {next:?} re-announced a state that did not change"
                            );
                            assert_eq!(event.kind, PlatformConnectionChanged::KIND);
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
                            assert_eq!(
                                event.payload["expected"].as_bool(),
                                Some(transition.is_expected()),
                                "{previous:?} -> {next:?} lost the {transition:?} flag"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_shared_announcement_finds_the_new_state_already_stored() {
        let state = Arc::new(AtomicConnectionState::new(ConnectionState::Disconnected));
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
            set_connection_state(&state, &*publisher, next, ConnectionTransition::Unexpected);
        }

        assert_eq!(publisher.observed.lock().unwrap().as_slice(), walk);
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
    fn diff_reports_a_change_only_when_the_expression_list_differs_from_the_baseline() {
        let list = |entries: &[(&str, bool)]| {
            json!({
                "expressions": entries
                    .iter()
                    .map(|(file, active)| json!({ "file": file, "active": active }))
                    .collect::<Vec<_>>()
            })
        };
        for (case, baseline, data, changed) in [
            ("first snapshot", None, list(&[("a", false)]), true),
            (
                "same list",
                Some(HashMap::from([("a".to_owned(), false)])),
                list(&[("a", false)]),
                false,
            ),
            (
                "active flip",
                Some(HashMap::from([("a".to_owned(), false)])),
                list(&[("a", true)]),
                true,
            ),
            (
                "added",
                Some(HashMap::from([("a".to_owned(), false)])),
                list(&[("a", false), ("b", false)]),
                true,
            ),
            (
                "removed",
                Some(HashMap::from([
                    ("a".to_owned(), false),
                    ("b".to_owned(), false),
                ])),
                list(&[("a", false)]),
                true,
            ),
        ] {
            let publisher = MockPublisher::new();
            let mut baseline = baseline;

            let reported = diff_and_emit_expressions(&data, &mut baseline, &*publisher.publisher());

            assert_eq!(reported, changed, "{case}");
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
    fn connection_changed_publishes_the_full_payload_with_null_for_absent_fields() {
        let publisher = MockPublisher::new();
        let p = publisher.publisher();
        emit_connection_changed(&*p, "ws://x:1", true, None, None);
        emit_connection_changed(
            &*p,
            "ws://x:1",
            false,
            Some(VTubeDisconnectReason::ConnectFailed),
            Some("dns failure".to_owned()),
        );

        let events = publisher.events.lock().unwrap();
        let published: Vec<_> = events
            .iter()
            .map(|e| (e.source, e.kind.as_str(), e.payload.clone()))
            .collect();
        assert_eq!(
            published,
            vec![
                (
                    EventSource::VTube,
                    "vtube.connection.changed",
                    json!({ "is_connected": true, "endpoint": "ws://x:1", "reason": null, "detail": null }),
                ),
                (
                    EventSource::VTube,
                    "vtube.connection.changed",
                    json!({
                        "is_connected": false,
                        "endpoint": "ws://x:1",
                        "reason": "connect_failed",
                        "detail": "dns failure",
                    }),
                ),
            ]
        );
    }

    mod stalled_writes {
        use crate::error::VTubeError;
        use crate::vts::socket::tests::{HANG_GUARD, SHORT_LIMIT, socket_with_a_stalled_write};

        use super::super::{request_new_token, send_seed_queries};

        #[tokio::test(start_paused = true)]
        async fn a_token_request_whose_write_stalls_ends_as_a_token_timeout() {
            let (mut ws, _peer) = socket_with_a_stalled_write().await;

            let outcome = tokio::time::timeout(
                HANG_GUARD,
                request_new_token(&mut ws, "ws://unused", SHORT_LIMIT),
            )
            .await
            .expect("the stalled write outlived its limit");

            assert!(
                matches!(outcome, Err(VTubeError::TokenTimeout)),
                "{outcome:?}"
            );
        }

        #[tokio::test(start_paused = true)]
        async fn seed_queries_whose_write_stalls_give_up_within_the_limit_with_nothing_awaited() {
            let (mut ws, _peer) = socket_with_a_stalled_write().await;

            let seeds =
                tokio::time::timeout(HANG_GUARD, send_seed_queries(&mut ws, SHORT_LIMIT)).await;

            assert!(seeds.expect("the stalled write hung").is_empty());
        }
    }

    mod dialing {
        use std::net::SocketAddr;
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        use forge_events::{PlatformConnectionChanged, TypedEvent};
        use forge_platform_core::{BuiltinControl, BuiltinStatus, ConnectionState};
        use tokio::net::{TcpListener, TcpStream};
        use tokio::sync::mpsc;
        use tokio::task::JoinHandle;

        use crate::client::tests::{MockCreds, MockPublisher, wait_until};
        use crate::client::{VTubeClient, VTubeConfig};
        use crate::timing::VTubeTiming;

        const HANG_GUARD: Duration = Duration::from_secs(30);
        const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);
        const SHORT_CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
        const QUIET_WINDOW: Duration = Duration::from_millis(50);

        struct DialCounter {
            addr: SocketAddr,
            dials: mpsc::UnboundedReceiver<SocketAddr>,
            server: JoinHandle<()>,
        }

        impl Drop for DialCounter {
            fn drop(&mut self) {
                self.server.abort();
            }
        }

        impl DialCounter {
            async fn serve(mut dials_to_drop: usize) -> Self {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                let (dialed, dials) = mpsc::unbounded_channel();
                let server = tokio::spawn(async move {
                    let mut held = Vec::new();
                    while let Ok((stream, from)) = listener.accept().await {
                        if dials_to_drop > 0 {
                            dials_to_drop -= 1;
                        } else {
                            held.push(stream);
                        }
                        if dialed.send(from).is_err() {
                            return;
                        }
                    }
                });
                Self {
                    addr,
                    dials,
                    server,
                }
            }

            fn connect(&self, publisher: &Arc<MockPublisher>, timing: VTubeTiming) -> VTubeClient {
                VTubeClient::connect_with_timing(
                    VTubeConfig {
                        endpoint: format!("ws://{}", self.addr),
                    },
                    publisher.publisher(),
                    MockCreds::new().creds(),
                    timing,
                )
            }

            async fn next_dial(&mut self) -> SocketAddr {
                tokio::time::timeout(HANG_GUARD, self.dials.recv())
                    .await
                    .ok()
                    .flatten()
                    .expect("forge stopped dialing")
            }

            async fn dials_before_a_sentinel(&mut self) -> usize {
                let sentinel = TcpStream::connect(self.addr).await.unwrap();
                let sentinel_addr = sentinel.local_addr().unwrap();
                let mut dials = 0;
                while self.next_dial().await != sentinel_addr {
                    dials += 1;
                }
                dials
            }
        }

        fn fast_redials() -> VTubeTiming {
            VTubeTiming {
                reconnect_base_delay: Duration::from_millis(1),
                reconnect_max_delay: Duration::from_millis(5),
                connect_timeout: SHORT_CONNECT_TIMEOUT,
                ..VTubeTiming::default()
            }
        }

        fn dial_story(publisher: &MockPublisher) -> Vec<String> {
            publisher
                .events
                .lock()
                .unwrap()
                .iter()
                .filter_map(|e| {
                    let field = if e.kind == PlatformConnectionChanged::KIND {
                        "state"
                    } else {
                        "reason"
                    };
                    e.payload[field].as_str().map(str::to_owned)
                })
                .collect()
        }

        fn failure_code(client: &VTubeClient) -> Option<String> {
            client
                .connect_failure()
                .map(|failure| failure.code().name().to_owned())
        }

        #[tokio::test]
        async fn a_dial_silent_on_the_upgrade_fails_and_redials_from_connecting_and_reconnecting() {
            let mut port = DialCounter::serve(0).await;
            let publisher = MockPublisher::new();
            let started = Instant::now();

            let client = port.connect(&publisher, fast_redials());
            for _ in 0..3 {
                port.next_dial().await;
            }

            assert_eq!(
                dial_story(&publisher)[..3],
                ["connect_failed", "reconnecting", "connect_failed"]
            );
            assert_eq!(failure_code(&client).as_deref(), Some("unreachable"));
            assert!(started.elapsed() >= 2 * SHORT_CONNECT_TIMEOUT);
        }

        #[tokio::test]
        async fn with_auto_reconnect_off_a_silent_dial_settles_disconnected_and_is_not_repeated() {
            let mut port = DialCounter::serve(0).await;
            let client = port.connect(&MockPublisher::new(), fast_redials());
            client.set_auto_reconnect(false);

            assert!(
                wait_until(HANG_GUARD, || client.connection_state()
                    == ConnectionState::Disconnected)
                .await,
                "the silent dial never settled"
            );
            tokio::time::sleep(QUIET_WINDOW).await;
            assert_eq!(failure_code(&client).as_deref(), Some("unreachable"));
            assert!(client.disconnect().await.is_ok());

            assert_eq!(port.dials_before_a_sentinel().await, 1);
        }

        #[tokio::test]
        async fn disconnecting_during_a_silent_upgrade_returns_at_once() {
            for (case, refused_dials) in [("first dial", 0), ("redial", 1)] {
                let mut port = DialCounter::serve(refused_dials).await;
                let client = port.connect(
                    &MockPublisher::new(),
                    VTubeTiming {
                        connect_timeout: BEYOND_ANY_GUARD,
                        ..fast_redials()
                    },
                );
                for _ in 0..=refused_dials {
                    port.next_dial().await;
                }

                let outcome = tokio::time::timeout(HANG_GUARD, client.disconnect()).await;

                assert!(
                    outcome.is_ok(),
                    "{case}: disconnect queued behind the upgrade"
                );
                assert_eq!(
                    client.connection_state(),
                    ConnectionState::Disconnected,
                    "{case}"
                );
            }
        }

        #[tokio::test]
        async fn the_base_delay_holds_back_the_next_dial() {
            let mut port = DialCounter::serve(usize::MAX).await;
            let publisher = MockPublisher::new();
            let client = port.connect(
                &publisher,
                VTubeTiming {
                    reconnect_base_delay: BEYOND_ANY_GUARD,
                    reconnect_max_delay: BEYOND_ANY_GUARD,
                    ..fast_redials()
                },
            );
            assert!(
                wait_until(HANG_GUARD, || publisher
                    .disconnected_with_reason("connect_failed"))
                .await,
                "precondition: the first dial never failed"
            );

            tokio::time::sleep(QUIET_WINDOW).await;
            assert!(client.disconnect().await.is_ok());

            assert_eq!(port.dials_before_a_sentinel().await, 1);
        }

        #[tokio::test]
        async fn the_max_delay_caps_a_longer_base_delay() {
            let mut port = DialCounter::serve(usize::MAX).await;
            let _client = port.connect(
                &MockPublisher::new(),
                VTubeTiming {
                    reconnect_base_delay: BEYOND_ANY_GUARD,
                    reconnect_max_delay: Duration::from_millis(1),
                    ..fast_redials()
                },
            );

            for _ in 0..2 {
                port.next_dial().await;
            }
        }
    }

    mod session {
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::time::Duration;

        use forge_platform_core::{
            BuiltinControl, BuiltinHealth, ConnectionState, HealthMetricId, HealthValue,
        };
        use serde_json::json;
        use tokio::sync::oneshot;

        use crate::client::VTubeClient;
        use crate::client::tests::{
            FakeVts, MockCreds, MockPublisher, PeerConn, stored_token_creds, wait_until,
        };
        use crate::sink::VTubeSink;
        use crate::timing::VTubeTiming;
        use crate::vts::socket::tests::{DoomedWrite, WriteFate};

        const SETTLE: Duration = Duration::from_secs(30);
        const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);
        const SHORT_REQUEST_TIMEOUT: Duration = Duration::from_millis(100);
        const SHORT_SWEEP_INTERVAL: Duration = Duration::from_millis(150);
        const STALLED_WRITE_LIMIT: Duration = Duration::from_millis(300);
        const SHORT_HANDSHAKE_TIMEOUT: Duration = Duration::from_millis(100);
        const FAST_POLL_INTERVAL: Duration = Duration::from_millis(5);
        const FIRST_POLL_TIMEOUT: Duration = Duration::from_millis(250);
        const ANSWERED_POLLS: usize = 10;
        const HOTKEY_REQUEST: &str = "HotkeyTriggerRequest";
        const EXPRESSION_POLL: &str = r#""details":false"#;

        struct CapturedLog {
            level: tracing::Level,
            fields: HashMap<String, String>,
        }

        impl tracing::field::Visit for CapturedLog {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.fields
                    .insert(field.name().to_owned(), format!("{value:?}"));
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                self.fields
                    .insert(field.name().to_owned(), value.to_owned());
            }
        }

        #[derive(Clone, Default)]
        struct LogCapture(Arc<Mutex<Vec<CapturedLog>>>);

        impl tracing::Subscriber for LogCapture {
            fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
                true
            }

            fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
                tracing::span::Id::from_u64(1)
            }

            fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

            fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

            fn event(&self, event: &tracing::Event<'_>) {
                let mut log = CapturedLog {
                    level: *event.metadata().level(),
                    fields: HashMap::new(),
                };
                event.record(&mut log);
                self.0.lock().unwrap().push(log);
            }

            fn enter(&self, _: &tracing::span::Id) {}

            fn exit(&self, _: &tracing::span::Id) {}
        }

        impl LogCapture {
            fn install() -> (Self, tracing::subscriber::DefaultGuard) {
                let capture = Self::default();
                let guard = tracing::subscriber::set_default(capture.clone());
                (capture, guard)
            }

            fn event_names_warned(&self) -> Vec<String> {
                self.0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|log| log.level == tracing::Level::WARN)
                    .filter_map(|log| log.fields.get("event_name").cloned())
                    .collect()
            }

            fn logged(&self, text: &str) -> bool {
                self.0.lock().unwrap().iter().any(|log| {
                    log.fields
                        .get("message")
                        .is_some_and(|message| message.contains(text))
                })
            }

            fn dropped_the_session_on_a_failed_write(&self) -> bool {
                self.logged("write failed")
            }
        }

        fn short_request_timeout() -> VTubeTiming {
            VTubeTiming {
                request_timeout: SHORT_REQUEST_TIMEOUT,
                request_sweep_interval: SHORT_SWEEP_INTERVAL,
                ..VTubeTiming::default()
            }
        }

        fn fast_redials(timing: VTubeTiming) -> VTubeTiming {
            VTubeTiming {
                reconnect_base_delay: Duration::from_millis(1),
                reconnect_max_delay: Duration::from_millis(5),
                ..timing
            }
        }

        async fn connected_session(
            vts: &mut FakeVts,
            publisher: &Arc<MockPublisher>,
        ) -> (VTubeClient, PeerConn) {
            connected_session_with_timing(vts, publisher, VTubeTiming::default()).await
        }

        async fn connected_session_with_timing(
            vts: &mut FakeVts,
            publisher: &Arc<MockPublisher>,
            timing: VTubeTiming,
        ) -> (VTubeClient, PeerConn) {
            let client = vts.connect_with_timing(publisher, &stored_token_creds(), timing);
            let conn = vts.logged_in_conn().await;
            assert!(
                wait_until(SETTLE, || client.connection_state().is_connected()).await,
                "the session never reached connected"
            );
            (client, conn)
        }

        async fn session_dooming_one_write(
            vts: &mut FakeVts,
            publisher: &Arc<MockPublisher>,
            timing: VTubeTiming,
            frame_fragment: &'static str,
            fate: WriteFate,
        ) -> (VTubeClient, PeerConn, oneshot::Receiver<()>) {
            let (reached, write_reached) = oneshot::channel();
            let doomed = DoomedWrite {
                frame_fragment,
                fate,
                reached,
            };
            let client =
                vts.connect_with_doomed_write(publisher, &stored_token_creds(), timing, doomed);
            let conn = vts.logged_in_conn().await;
            (client, conn, write_reached)
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

        fn metric(client: &VTubeClient, id: &HealthMetricId) -> HealthValue {
            client
                .metrics()
                .into_iter()
                .find(|m| &m.id == id)
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

        async fn serve_every_frame(mut conn: PeerConn) {
            while let Some(frame) = conn.next_frame().await {
                let answer = if frame["messageType"] == "AuthenticationRequest" {
                    json!({ "authenticated": true, "reason": "" })
                } else {
                    json!({})
                };
                conn.tx.reply(&frame, answer);
            }
        }

        #[tokio::test]
        async fn two_overlapping_reconnects_leave_one_session_that_serves_requests() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, _retired) = connected_session(&mut vts, &publisher).await;

            let (first, second) = tokio::join!(client.reconnect(), client.reconnect());
            assert!(first.is_ok() && second.is_ok());

            let accept_every_dial = async {
                while let Some(conn) = vts.next_conn(SETTLE).await {
                    tokio::spawn(serve_every_frame(conn));
                }
            };
            let served = async {
                assert!(
                    wait_until(SETTLE, || client.connection_state().is_connected()).await,
                    "no session came back after the overlapping reconnects"
                );
                client.trigger_hotkey("hk-1").await
            };
            let outcome = tokio::select! {
                outcome = served => outcome,
                () = accept_every_dial => panic!("forge stopped dialing"),
            };

            assert!(
                outcome.is_ok(),
                "requests went to a retired session: {outcome:?}"
            );
        }

        #[tokio::test]
        async fn a_request_write_that_fails_ends_the_session_and_redials() {
            let (log, _subscriber) = LogCapture::install();
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, _conn, _reached) = session_dooming_one_write(
                &mut vts,
                &publisher,
                write_stall_timing(BEYOND_ANY_GUARD),
                HOTKEY_REQUEST,
                WriteFate::Fail,
            )
            .await;
            assert!(
                wait_until(SETTLE, || client.connection_state().is_connected()).await,
                "the session never reached connected"
            );

            let outcome = client.trigger_hotkey("hk-1").await;

            assert!(outcome.is_err(), "the failed write was reported as sent");
            assert!(
                vts.next_redial(SETTLE).await.is_some(),
                "a session lost to a failed write must be redialed"
            );
            assert!(
                log.dropped_the_session_on_a_failed_write(),
                "the session outlived its failed write"
            );
        }

        #[tokio::test]
        async fn a_rejected_event_subscription_is_logged_by_name_and_keeps_the_session() {
            let (log, _subscriber) = LogCapture::install();
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, mut conn) = connected_session(&mut vts, &publisher).await;
            let accepted = conn.expect("EventSubscriptionRequest", SETTLE).await;
            let rejected = conn.expect("EventSubscriptionRequest", SETTLE).await;
            let rejected_name = rejected["data"]["eventName"].as_str().unwrap().to_owned();

            conn.tx.reply(&accepted, json!({}));
            conn.tx.api_error(&rejected, 50, "unknown event");

            assert!(
                wait_until(SETTLE, || !log.event_names_warned().is_empty()).await,
                "the rejected subscription was never logged"
            );
            assert_eq!(log.event_names_warned(), vec![rejected_name]);
            assert!(client.connection_state().is_connected());
        }

        #[tokio::test]
        async fn a_request_left_unanswered_drops_the_session_as_unresponsive_and_redials() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, conn) =
                connected_session_with_timing(&mut vts, &publisher, short_request_timeout()).await;
            let (_peer, ignored) =
                conn.answer_all_except(|f| f["messageType"] == "HotkeyTriggerRequest");

            let (_outcome, ()) = tokio::join!(client.trigger_hotkey("hk-1"), async {
                assert!(
                    wait_until(SETTLE, || ignored.load(Ordering::SeqCst) == 1).await,
                    "the hotkey request never reached VTS"
                );
            });

            assert!(
                wait_until(SETTLE, || publisher
                    .disconnected_with_reason("unresponsive"))
                .await,
                "a request past its deadline must drop the session as unresponsive"
            );
            assert!(
                vts.next_redial(SETTLE).await.is_some(),
                "an unresponsive session must be redialed"
            );
        }

        #[tokio::test]
        async fn an_unanswered_request_times_out_the_caller_before_dropping_the_session() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let timing = short_request_timeout();
            let (client, conn) = connected_session_with_timing(&mut vts, &publisher, timing).await;
            let (_peer, ignored) =
                conn.answer_all_except(|f| f["messageType"] == "HotkeyTriggerRequest");
            let started = std::time::Instant::now();

            let (outcome, ()) = tokio::join!(client.trigger_hotkey("hk-1"), async {
                assert!(
                    wait_until(SETTLE, || ignored.load(Ordering::SeqCst) == 1).await,
                    "the hotkey request never reached VTS"
                );
            });

            assert!(
                matches!(outcome, Err(crate::error::VTubeError::Timeout)),
                "the caller must time out while the session still stands, got {outcome:?}"
            );
            assert!(
                wait_until(SETTLE, || publisher
                    .disconnected_with_reason("unresponsive"))
                .await,
                "the first sweep past the unresponsive threshold must drop the session"
            );
            assert!(
                started.elapsed() >= timing.unresponsive_threshold(),
                "the session was dropped before the unresponsive threshold"
            );
        }

        #[tokio::test]
        async fn a_request_write_that_stalls_drops_the_session_and_redials() {
            let (log, _subscriber) = LogCapture::install();
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, _conn, _reached) = session_dooming_one_write(
                &mut vts,
                &publisher,
                write_stall_timing(SHORT_REQUEST_TIMEOUT),
                HOTKEY_REQUEST,
                WriteFate::Stall,
            )
            .await;
            assert!(
                wait_until(SETTLE, || client.connection_state().is_connected()).await,
                "the session never reached connected"
            );

            let outcome = client.trigger_hotkey("hk-1").await;

            assert!(outcome.is_err(), "the stalled write was reported as sent");
            assert!(
                vts.next_redial(SETTLE).await.is_some(),
                "a session lost to a stalled write must be redialed"
            );
            assert!(log.logged("write stalled"), "the write never stalled");
        }

        fn write_stall_timing(request_timeout: Duration) -> VTubeTiming {
            fast_redials(VTubeTiming {
                request_timeout,
                request_sweep_interval: BEYOND_ANY_GUARD,
                expression_poll_timeout: BEYOND_ANY_GUARD,
                ..VTubeTiming::default()
            })
        }

        fn poll_stall_timing(request_timeout: Duration) -> VTubeTiming {
            VTubeTiming {
                expression_poll_interval: FAST_POLL_INTERVAL,
                ..write_stall_timing(request_timeout)
            }
        }

        #[tokio::test]
        async fn an_expression_poll_write_that_stalls_or_fails_drops_the_session_and_redials() {
            for (case, request_timeout, fate) in [
                ("stalled", SHORT_REQUEST_TIMEOUT, WriteFate::Stall),
                ("failed", BEYOND_ANY_GUARD, WriteFate::Fail),
            ] {
                let (log, _subscriber) = LogCapture::install();
                let mut vts = FakeVts::bind().await;
                let publisher = MockPublisher::new();
                let (_client, _conn, _reached) = session_dooming_one_write(
                    &mut vts,
                    &publisher,
                    poll_stall_timing(request_timeout),
                    EXPRESSION_POLL,
                    fate,
                )
                .await;

                assert!(
                    vts.next_redial(SETTLE).await.is_some(),
                    "{case}: a session lost to its poll write must be redialed"
                );
                assert!(
                    log.dropped_the_session_on_a_failed_write(),
                    "{case}: the session must end on its poll write"
                );
            }
        }

        #[tokio::test]
        async fn disconnecting_during_a_stalled_expression_poll_write_ends_within_its_limit() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, _conn, stalled) = session_dooming_one_write(
                &mut vts,
                &publisher,
                poll_stall_timing(STALLED_WRITE_LIMIT),
                EXPRESSION_POLL,
                WriteFate::Stall,
            )
            .await;
            stalled.await.expect("the poll write was never attempted");

            let outcome = tokio::time::timeout(SETTLE, client.disconnect()).await;

            assert!(outcome.is_ok(), "disconnect outlived the stalled write");
            assert_eq!(client.connection_state(), ConnectionState::Disconnected);
        }

        #[tokio::test]
        async fn a_peer_silent_after_the_upgrade_fails_the_handshake_at_the_reply_deadline() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let started = std::time::Instant::now();
            let _client = vts.connect_with_timing(
                &publisher,
                &stored_token_creds(),
                VTubeTiming {
                    handshake_reply_timeout: SHORT_HANDSHAKE_TIMEOUT,
                    ..VTubeTiming::default()
                },
            );
            let mut conn = vts.next_conn(SETTLE).await.unwrap();
            conn.expect("AuthenticationRequest", SETTLE).await;

            let failed =
                wait_until(SETTLE, || publisher.disconnected_with_reason("auth_failed")).await;

            assert!(failed, "a login nobody answers must end as auth_failed");
            assert!(
                started.elapsed() >= SHORT_HANDSHAKE_TIMEOUT,
                "the login was given up before the handshake deadline"
            );
            drop(conn);
        }

        #[tokio::test]
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
                let client = vts.connect_with_timing(
                    &MockPublisher::new(),
                    &creds,
                    VTubeTiming {
                        handshake_reply_timeout: BEYOND_ANY_GUARD,
                        token_approval_timeout: BEYOND_ANY_GUARD,
                        ..VTubeTiming::default()
                    },
                );
                let mut conn = vts.next_conn(SETTLE).await.unwrap();
                conn.expect(request, SETTLE).await;

                let outcome = tokio::time::timeout(SETTLE, client.disconnect()).await;

                assert!(
                    outcome.is_ok(),
                    "disconnect during the {phase} waited for the peer instead of cancelling"
                );
                drop(conn);
            }
        }

        async fn peer_reporting_a_hotkey_on_each_unanswered_poll(
            mut conn: PeerConn,
            polls_seen: Arc<AtomicUsize>,
        ) {
            while let Some(frame) = conn.next_frame().await {
                if is_expression_poll(&frame) {
                    let poll = polls_seen.fetch_add(1, Ordering::SeqCst) + 1;
                    conn.tx.event(
                        "HotkeyTriggeredEvent",
                        json!({ "hotkeyID": format!("hk-{poll}"), "hotkeyName": "Wave" }),
                    );
                } else {
                    conn.tx.reply(&frame, json!({}));
                }
            }
        }

        #[tokio::test]
        async fn two_unanswered_expression_polls_drop_the_session_even_while_events_flow() {
            let mut poll_timeout = FIRST_POLL_TIMEOUT;
            loop {
                let mut vts = FakeVts::bind().await;
                let publisher = MockPublisher::new();
                let timing = VTubeTiming {
                    expression_poll_interval: FAST_POLL_INTERVAL,
                    expression_poll_timeout: poll_timeout,
                    ..VTubeTiming::default()
                };
                let (_client, conn) =
                    connected_session_with_timing(&mut vts, &publisher, timing).await;
                let polls_seen = Arc::new(AtomicUsize::new(0));
                tokio::spawn(peer_reporting_a_hotkey_on_each_unanswered_poll(
                    conn,
                    Arc::clone(&polls_seen),
                ));

                assert!(
                    wait_until(SETTLE, || publisher
                        .disconnected_with_reason("unresponsive"))
                    .await,
                    "a peer that stops answering polls must be dropped"
                );
                assert_eq!(
                    polls_seen.load(Ordering::SeqCst),
                    2,
                    "the session must drop on the second unanswered poll"
                );
                if hotkey_event_seen(&publisher, "hk-1") && hotkey_event_seen(&publisher, "hk-2") {
                    return;
                }
                poll_timeout *= 2;
                assert!(poll_timeout < SETTLE, "the events never arrived in time");
            }
        }

        #[tokio::test]
        async fn a_credential_read_failure_redials_without_asking_the_user_to_pair_again() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let creds = stored_token_creds();
            creds.refuse_next_read();
            let client =
                vts.connect_with_timing(&publisher, &creds, fast_redials(VTubeTiming::default()));

            let mut refused = vts.next_conn(SETTLE).await.unwrap();
            let frame = tokio::time::timeout(SETTLE, refused.next_frame())
                .await
                .unwrap();
            let mut redialed = vts.next_redial(SETTLE).await.unwrap();
            redialed.accept_login().await;

            assert!(frame.is_none(), "forge sent {frame:?} without a credential");
            assert!(
                wait_until(SETTLE, || client.connection_state().is_connected()).await,
                "the stored token never logged in after the read recovered"
            );
        }

        fn is_model_list(frame: &serde_json::Value) -> bool {
            frame["messageType"] == "AvailableModelsRequest"
        }

        fn is_hotkey_list(frame: &serde_json::Value) -> bool {
            frame["messageType"] == "HotkeysInCurrentModelRequest"
        }

        fn is_item_list(frame: &serde_json::Value) -> bool {
            frame["messageType"] == "ItemListRequest"
        }

        fn is_expression_sweep(frame: &serde_json::Value) -> bool {
            frame["messageType"] == "ExpressionStateRequest" && frame["data"]["details"].is_null()
        }

        async fn next_frame_answering_polls(
            conn: &mut PeerConn,
            poll_answer: &serde_json::Value,
        ) -> serde_json::Value {
            loop {
                let frame = tokio::time::timeout(SETTLE, conn.next_frame())
                    .await
                    .ok()
                    .flatten()
                    .expect("the session must keep sending requests");
                if !is_expression_poll(&frame) {
                    return frame;
                }
                conn.tx.reply(&frame, poll_answer.clone());
            }
        }

        async fn hold_frame(
            conn: &mut PeerConn,
            is_wanted: fn(&serde_json::Value) -> bool,
            poll_answer: &serde_json::Value,
        ) -> serde_json::Value {
            loop {
                let frame = next_frame_answering_polls(conn, poll_answer).await;
                if is_wanted(&frame) {
                    return frame;
                }
                conn.tx.reply(&frame, json!({}));
            }
        }

        async fn serve_past(
            conn: &mut PeerConn,
            is_wanted: fn(&serde_json::Value) -> bool,
            poll_answer: &serde_json::Value,
        ) {
            let frame = hold_frame(conn, is_wanted, poll_answer).await;
            conn.tx.reply(&frame, json!({}));
        }

        async fn serve_past_one_poll(conn: &mut PeerConn, poll_answer: &serde_json::Value) {
            loop {
                let frame = tokio::time::timeout(SETTLE, conn.next_frame())
                    .await
                    .ok()
                    .flatten()
                    .expect("the session must keep polling");
                if is_expression_poll(&frame) {
                    conn.tx.reply(&frame, poll_answer.clone());
                    return;
                }
                conn.tx.reply(&frame, json!({}));
            }
        }

        #[tokio::test]
        async fn a_model_config_change_refreshes_the_catalog_again() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (_client, mut conn) = connected_session(&mut vts, &publisher).await;
            let no_expressions = json!({});
            serve_past(&mut conn, is_model_list, &no_expressions).await;
            serve_past(&mut conn, is_model_list, &no_expressions).await;

            conn.tx.event(
                "ModelConfigChangedEvent",
                json!({ "modelID": "m-1", "modelName": "MyAvatar" }),
            );

            serve_past(&mut conn, is_model_list, &no_expressions).await;
        }

        #[tokio::test]
        async fn an_expression_poll_that_differs_from_the_last_one_starts_a_catalog_sweep() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let timing = VTubeTiming {
                expression_poll_interval: FAST_POLL_INTERVAL,
                request_timeout: SETTLE,
                ..VTubeTiming::default()
            };
            let (_client, mut conn) =
                connected_session_with_timing(&mut vts, &publisher, timing).await;
            let list = |active: bool| json!({ "expressions": [{ "file": "smile.exp3.json", "active": active }] });
            let idle = list(false);
            let smiling = list(true);

            let held_sweep = hold_frame(&mut conn, is_expression_sweep, &idle).await;
            serve_past(&mut conn, is_hotkey_list, &idle).await;
            serve_past(&mut conn, is_hotkey_list, &idle).await;
            serve_past_one_poll(&mut conn, &idle).await;
            serve_past_one_poll(&mut conn, &idle).await;
            conn.tx.reply(&held_sweep, json!({}));
            serve_past(&mut conn, is_item_list, &idle).await;
            serve_past(&mut conn, is_item_list, &idle).await;
            serve_past_one_poll(&mut conn, &idle).await;

            serve_past_one_poll(&mut conn, &smiling).await;

            serve_past(&mut conn, is_expression_sweep, &smiling).await;
        }

        #[tokio::test]
        async fn a_peer_that_answers_every_poll_stays_connected() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let timing = VTubeTiming {
                expression_poll_interval: FAST_POLL_INTERVAL,
                ..VTubeTiming::default()
            };
            let (client, mut conn) =
                connected_session_with_timing(&mut vts, &publisher, timing).await;

            let mut polls = 0;
            while polls <= ANSWERED_POLLS {
                let next = tokio::time::timeout(SETTLE, conn.next_frame())
                    .await
                    .ok()
                    .flatten();
                assert!(
                    next.is_some(),
                    "forge dropped or stopped polling a peer that answers every request"
                );
                let frame = next.unwrap();
                conn.tx.reply(&frame, json!({}));
                if is_expression_poll(&frame) {
                    polls += 1;
                }
            }

            assert!(
                publisher.disconnected_event().is_none(),
                "a responsive peer was dropped"
            );
            assert!(client.connection_state().is_connected());
        }

        #[tokio::test]
        async fn the_current_model_and_face_replies_seed_the_model_and_tracking_tiles() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, mut conn) = connected_session(&mut vts, &publisher).await;
            let mut deltas = client.health_tx.subscribe();

            answer_seeds(&mut conn).await;

            assert!(
                wait_until(SETTLE, || matches!(
                    metric(&client, &crate::health::TRACKING_METRIC),
                    HealthValue::Status { active: true, .. }
                ))
                .await,
                "the face seed never reached the TRACKING tile"
            );
            let model_delta = std::iter::from_fn(|| deltas.try_recv().ok())
                .find(|d| d.metric == crate::health::MODEL_METRIC)
                .map(|d| d.new_value);
            assert_eq!(
                model_delta,
                Some(crate::health::model_value("MyAvatar", Some(42)))
            );
        }

        #[tokio::test]
        async fn a_session_end_clears_the_model_and_tracking_tiles() {
            let mut vts = FakeVts::bind().await;
            let publisher = MockPublisher::new();
            let (client, mut conn) = connected_session(&mut vts, &publisher).await;
            answer_seeds(&mut conn).await;
            assert!(
                wait_until(SETTLE, || matches!(
                    metric(&client, &crate::health::TRACKING_METRIC),
                    HealthValue::Status { active: true, .. }
                ))
                .await,
                "precondition: the seeds must land first"
            );

            conn.tx.close();

            assert!(
                wait_until(SETTLE, || publisher
                    .disconnected_with_reason("socket_closed"))
                .await,
                "precondition: the session must end"
            );
            assert_eq!(
                (
                    metric(&client, &crate::health::MODEL_METRIC),
                    metric(&client, &crate::health::TRACKING_METRIC)
                ),
                (
                    crate::health::model_value("", None),
                    HealthValue::Status {
                        label: crate::vocabulary::messages::health_tracking(false).into(),
                        active: false,
                        detail: None,
                    },
                )
            );
        }

        #[tokio::test]
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
                    wait_until(SETTLE, || hotkey_event_seen(&publisher, "sentinel")).await,
                    "{case}: the events were never processed"
                );

                assert_eq!(face_kinds(&publisher), expected, "{case}");
            }
        }
    }
}
