use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::payload_fields::connection as connection_fields;
use crate::payload_fields::expression as expression_fields;
use crate::protocol::new_request;
use crate::request::PendingRequest;

const VTS_BACKOFF_CAP: Duration = Duration::from_secs(30);
const EXPRESSION_POLL_INTERVAL: Duration = Duration::from_secs(3);
const EXPRESSION_POLL_TIMEOUT: Duration = Duration::from_secs(5);

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

async fn await_expression_response(
    pending: &mut Option<tokio::sync::oneshot::Receiver<serde_json::Value>>,
) -> Option<serde_json::Value> {
    match pending.as_mut() {
        Some(rx) => match tokio::time::timeout(EXPRESSION_POLL_TIMEOUT, rx).await {
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

async fn request_new_token(ws: &mut VtsWs, endpoint: &str) -> Result<String, VTubeError> {
    let req = new_request(
        "AuthenticationTokenRequest",
        serde_json::json!({
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME
        }),
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
            "pluginName": crate::PLUGIN_NAME,
            "pluginDeveloper": crate::PLUGIN_NAME,
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

pub(crate) fn set_connection_state(
    state: &AtomicConnectionState,
    health_state: &RwLock<HealthSnapshot>,
    new_state: ConnectionState,
) {
    state.store(new_state);
    let dialing = matches!(
        new_state,
        ConnectionState::Connecting | ConnectionState::Reconnecting
    );
    if let Ok(mut g) = health_state.write() {
        g.dialing = dialing;
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
                    set_connection_state(&state, &health_state, ConnectionState::Disconnected);
                    emit_connection_changed(&*publisher, &endpoint, false, None, None);
                    return;
                }
            }
            if !auto_reconnect.load(Ordering::Relaxed) {
                set_connection_state(&state, &health_state, ConnectionState::Disconnected);
                emit_connection_changed(&*publisher, &endpoint, false, None, None);
                return;
            }
        }

        set_connection_state(
            &state,
            &health_state,
            if reconnecting {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Connecting
            },
        );
        tracing::debug!(endpoint = %endpoint, "attempting VTube Studio connection");

        let mut ws = match tokio_tungstenite::connect_async(&endpoint).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                tracing::debug!(
                    endpoint = %endpoint,
                    error = %e,
                    "VTube Studio connection attempt failed"
                );
                let retry = auto_reconnect.load(Ordering::Relaxed);
                if !retry {
                    set_connection_state(&state, &health_state, ConnectionState::Disconnected);
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

        match run_auth(&mut ws, &*creds, &endpoint, &auth_state, &*publisher).await {
            Ok(()) => {}
            Err(VTubeError::TokenRejected) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(&state, &health_state, ConnectionState::Disconnected);
                emit_connection_changed(&*publisher, &endpoint, false, Some("auth_required"), None);
                return;
            }
            Err(VTubeError::TokenDenied) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(&state, &health_state, ConnectionState::Disconnected);
                emit_connection_changed(&*publisher, &endpoint, false, Some("auth_denied"), None);
                return;
            }
            Err(VTubeError::TokenTimeout) => {
                if let Ok(mut g) = auth_state.write() {
                    *g = AuthState::AuthRequired;
                }
                set_connection_state(&state, &health_state, ConnectionState::Disconnected);
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
                    set_connection_state(&state, &health_state, ConnectionState::Disconnected);
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

        if let Err(e) = crate::events::subscribe_all(&mut ws).await {
            tracing::debug!(endpoint = %endpoint, error = %e, "event subscription failed, will retry");
            let retry = auto_reconnect.load(Ordering::Relaxed);
            if !retry {
                set_connection_state(&state, &health_state, ConnectionState::Disconnected);
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
        set_connection_state(&state, &health_state, ConnectionState::Connected);
        emit_connection_changed(&*publisher, &endpoint, true, None, None);
        let _ = connected_notifier.send(());
        tracing::info!(endpoint = %endpoint, "connected and authenticated to VTube Studio");

        let mut pending: HashMap<String, tokio::sync::oneshot::Sender<serde_json::Value>> =
            HashMap::new();
        let mut expr_tick = tokio::time::interval(EXPRESSION_POLL_INTERVAL);
        let mut expr_baseline: Option<HashMap<String, bool>> = None;
        let mut expr_pending: Option<tokio::sync::oneshot::Receiver<serde_json::Value>> = None;

        loop {
            tokio::select! {
                () = shutdown.notified() => {
                    set_connection_state(&state, &health_state, ConnectionState::Disconnected);
                    if let Ok(mut g) = auth_state.write() {
                        *g = AuthState::Cold;
                    }
                    emit_connection_changed(&*publisher, &endpoint, false, None, None);
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
                                        expr_baseline = None;
                                    }
                                    crate::events::dispatch_vts_event(&env, &*publisher);
                                    update_from_event(&env, &health_state, &health_tx);
                                }
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
                _ = expr_tick.tick(), if expr_pending.is_none() => {
                    let req = new_request("ExpressionStateRequest", serde_json::json!({ "details": false }));
                    let request_id = req.request_id.clone();
                    if let Ok(text) = serde_json::to_string(&req) {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        if ws.send(Message::Text(text.into())).await.is_ok() {
                            pending.insert(request_id, tx);
                            expr_pending = Some(rx);
                        }
                    }
                }
                result = await_expression_response(&mut expr_pending) => {
                    expr_pending = None;
                    if let Some(data) = result {
                        diff_and_emit_expressions(&data, &mut expr_baseline, &*publisher);
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
        let retry = auto_reconnect.load(Ordering::Relaxed);
        set_connection_state(
            &state,
            &health_state,
            if retry {
                ConnectionState::Reconnecting
            } else {
                ConnectionState::Disconnected
            },
        );
        emit_connection_changed(&*publisher, &endpoint, false, Some("socket_closed"), None);
        if !retry {
            return;
        }
        backoff.reset();
        reconnecting = true;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{diff_and_emit_expressions, emit_connection_changed};
    use crate::client::tests::MockPublisher;

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
}
