#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::mpsc;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::backend::{HotkeyBackend, HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

pub(crate) struct PortalBackend {
    cmd_tx: mpsc::Sender<PortalCmd>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
    restart_rx_slot: Mutex<Option<mpsc::Receiver<()>>>,
}

enum PortalCmd {
    Register(HotkeyId, HotkeyCombo),
    Unregister(HotkeyId),
}

impl PortalBackend {
    pub(crate) async fn try_new(app_name: &str) -> Result<Self, HotkeyError> {
        if std::env::var("WAYLAND_DISPLAY").is_err() {
            return Err(HotkeyError::PortalUnavailable {
                reason: "not a Wayland session".to_owned(),
            });
        }

        let conn = Connection::session()
            .await
            .map_err(|e| HotkeyError::PortalUnavailable {
                reason: format!("D-Bus session unavailable: {e}"),
            })?;

        register_host_app_id(&conn, app_name).await;

        let session_path = create_portal_session(&conn, app_name).await.map_err(|e| {
            HotkeyError::PortalUnavailable {
                reason: format!("GlobalShortcuts portal not available: {e}"),
            }
        })?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<PortalCmd>(64);
        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(64);
        let (restart_notice_tx, restart_notice_rx) = mpsc::channel::<()>(4);

        tokio::spawn(run_portal_task(
            conn,
            session_path,
            app_name.to_owned(),
            cmd_rx,
            fired_tx,
            restart_notice_tx,
        ));

        Ok(Self {
            cmd_tx,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
            restart_rx_slot: Mutex::new(Some(restart_notice_rx)),
        })
    }
}

impl HotkeyBackend for PortalBackend {
    fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        self.cmd_tx
            .try_send(PortalCmd::Register(id, combo.clone()))
            .map_err(|e| HotkeyError::Backend(e.to_string()))
    }

    fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        self.cmd_tx
            .try_send(PortalCmd::Unregister(id))
            .map_err(|e| HotkeyError::Backend(e.to_string()))
    }

    fn fired_rx(&self) -> Option<mpsc::Receiver<HotkeyFiredEvent>> {
        self.fired_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }

    fn delivery_gate_only(&self) -> bool {
        true
    }

    fn restart_rx(&self) -> Option<mpsc::Receiver<()>> {
        self.restart_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

async fn register_host_app_id(conn: &Connection, app_id: &str) {
    let proxy = match zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.host.portal.Registry",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "host registry portal absent; continuing without app id");
            return;
        }
    };

    let options: HashMap<&str, Value<'_>> = HashMap::new();
    if let Err(e) = proxy.call::<_, _, ()>("Register", &(app_id, options)).await {
        tracing::debug!(error = %e, "host app id registration skipped");
    }
}

async fn create_portal_session(conn: &Connection, app_name: &str) -> zbus::Result<OwnedObjectPath> {
    use tokio_stream::StreamExt as TokioStreamExt;

    let handle_token = make_token(app_name, "session");
    let session_token = make_token(app_name, "sess");

    let unique = conn
        .unique_name()
        .map(|n| n.as_str().to_owned())
        .unwrap_or_default();
    let sender = unique.trim_start_matches(':').replace('.', "_");
    let request_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{handle_token}");

    let request_proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .await?;
    let response_stream = request_proxy.receive_signal("Response").await?;
    let mut response_stream = std::pin::pin!(response_stream);

    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.GlobalShortcuts",
    )
    .await?;

    let options: HashMap<&str, Value<'_>> = [
        ("handle_token", Value::Str(handle_token.as_str().into())),
        (
            "session_handle_token",
            Value::Str(session_token.as_str().into()),
        ),
    ]
    .into_iter()
    .collect();

    let _request: OwnedObjectPath = proxy.call("CreateSession", &(options,)).await?;

    let msg = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        TokioStreamExt::next(&mut response_stream),
    )
    .await
    .map_err(|_| zbus::Error::Failure("CreateSession response timed out".to_owned()))?
    .ok_or_else(|| zbus::Error::Failure("Response signal stream closed".to_owned()))?;

    let (response, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize()?;

    if response != 0 {
        return Err(zbus::Error::Failure(format!(
            "CreateSession request returned non-zero response: {response}"
        )));
    }

    let session_handle = results
        .get("session_handle")
        .ok_or_else(|| zbus::Error::Failure("missing session_handle".to_owned()))?;

    extract_object_path(session_handle)
        .ok_or_else(|| zbus::Error::Failure("session_handle is not an object path".to_owned()))
}

fn extract_object_path(v: &OwnedValue) -> Option<OwnedObjectPath> {
    match &**v {
        Value::ObjectPath(p) => OwnedObjectPath::try_from(p.as_str()).ok(),
        Value::Str(s) => OwnedObjectPath::try_from(s.as_str()).ok(),
        _ => None,
    }
}

fn make_token(app_name: &str, suffix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{}_{suffix}_{ts}", app_name.replace('-', "_"))
}

async fn run_portal_task(
    conn: Connection,
    session_path: OwnedObjectPath,
    app_name: String,
    mut cmd_rx: mpsc::Receiver<PortalCmd>,
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
    restart_notice_tx: mpsc::Sender<()>,
) {
    let mut registered: HashMap<HotkeyId, HotkeyCombo> = HashMap::new();
    let mut combo_to_id: HashMap<String, HotkeyId> = HashMap::new();

    let (edge_tx, mut edge_rx) = mpsc::channel::<(String, u64, HotkeyEdge)>(64);
    let (restart_tx, mut restart_rx) = mpsc::channel::<()>(4);

    let conn_clone = conn.clone();
    let session_path_clone = session_path.clone();
    let app_name_clone = app_name.clone();
    let edge_tx_clone = edge_tx.clone();
    let restart_tx_clone = restart_tx.clone();

    tokio::spawn(signal_listener_task(
        conn_clone,
        session_path_clone,
        app_name_clone,
        edge_tx_clone,
        restart_tx_clone,
    ));

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(PortalCmd::Register(id, combo)) => {
                        combo_to_id.insert(combo.as_str().to_owned(), id);
                        registered.insert(id, combo);
                        bind_all_shortcuts(&conn, &session_path, &registered).await;
                    }
                    Some(PortalCmd::Unregister(id)) => {
                        if let Some(combo) = registered.remove(&id) {
                            combo_to_id.remove(combo.as_str());
                        }
                        bind_all_shortcuts(&conn, &session_path, &registered).await;
                    }
                    None => break,
                }
            }
            Some((shortcut_id, timestamp, edge)) = edge_rx.recv() => {
                if let Some(&id) = combo_to_id.get(&shortcut_id)
                    && let Ok(combo) = HotkeyCombo::parse(&shortcut_id)
                {
                    let _ = fired_tx.send(HotkeyFiredEvent {
                        id,
                        combo,
                        timestamp_us: timestamp,
                        edge,
                    }).await;
                }
            }
            Some(()) = restart_rx.recv() => {
                tracing::info!("portal daemon restarted - recreating session");
                let _ = restart_notice_tx.send(()).await;
                let conn_clone = conn.clone();
                let app_name_clone = app_name.clone();
                let edge_tx_clone = edge_tx.clone();
                let restart_tx_clone = restart_tx.clone();
                tokio::spawn(signal_listener_task(
                    conn_clone,
                    session_path.clone(),
                    app_name_clone,
                    edge_tx_clone,
                    restart_tx_clone,
                ));
                bind_all_shortcuts(&conn, &session_path, &registered).await;
            }
        }
    }
}

async fn signal_listener_task(
    conn: Connection,
    _session_path: OwnedObjectPath,
    _app_name: String,
    edge_tx: mpsc::Sender<(String, u64, HotkeyEdge)>,
    restart_tx: mpsc::Sender<()>,
) {
    use tokio_stream::StreamExt as TokioStreamExt;

    let activated_proxy = match zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.GlobalShortcuts",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "failed to create portal proxy for signals");
            return;
        }
    };

    let dbus_proxy = match zbus::fdo::DBusProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "failed to create DBus proxy");
            return;
        }
    };

    let activated_stream = match activated_proxy.receive_signal("Activated").await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to Activated signal");
            return;
        }
    };

    let deactivated_stream = match activated_proxy.receive_signal("Deactivated").await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to Deactivated signal");
            return;
        }
    };

    let name_owner_stream = match dbus_proxy.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to NameOwnerChanged");
            return;
        }
    };

    let mut activated_stream = std::pin::pin!(activated_stream);
    let mut deactivated_stream = std::pin::pin!(deactivated_stream);
    let mut name_owner_stream = std::pin::pin!(name_owner_stream);

    loop {
        tokio::select! {
            maybe_msg = TokioStreamExt::next(&mut activated_stream) => {
                let Some(msg) = maybe_msg else { break };
                if let Some((shortcut_id, timestamp)) = parse_shortcut_signal(&msg) {
                    let _ = edge_tx.send((shortcut_id, timestamp, HotkeyEdge::Press)).await;
                }
            }
            maybe_msg = TokioStreamExt::next(&mut deactivated_stream) => {
                let Some(msg) = maybe_msg else { break };
                if let Some((shortcut_id, timestamp)) = parse_shortcut_signal(&msg) {
                    let _ = edge_tx.send((shortcut_id, timestamp, HotkeyEdge::Release)).await;
                }
            }
            maybe_change = TokioStreamExt::next(&mut name_owner_stream) => {
                let Some(change) = maybe_change else { break };
                if let Ok(args) = change.args()
                    && args.name() == "org.freedesktop.portal.Desktop"
                    && !args.new_owner().as_deref().unwrap_or("").is_empty()
                {
                    let _ = restart_tx.send(()).await;
                    return;
                }
            }
        }
    }
}

fn parse_shortcut_signal(msg: &zbus::Message) -> Option<(String, u64)> {
    let body: (OwnedObjectPath, String, u64, HashMap<String, OwnedValue>) =
        msg.body().deserialize().ok()?;
    let (_session, shortcut_id, timestamp, _opts) = body;
    Some((shortcut_id, timestamp))
}

async fn bind_all_shortcuts(
    conn: &Connection,
    session_path: &OwnedObjectPath,
    registered: &HashMap<HotkeyId, HotkeyCombo>,
) {
    let proxy = match zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.GlobalShortcuts",
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "failed to create proxy for BindShortcuts");
            return;
        }
    };

    let shortcuts: Vec<(String, HashMap<&str, Value<'_>>)> = registered
        .values()
        .map(|combo| {
            let mut props: HashMap<&str, Value<'_>> = HashMap::new();
            props.insert("description", Value::Str(combo.as_str().into()));
            (combo.as_str().to_owned(), props)
        })
        .collect();

    let shortcut_refs: Vec<(&str, &HashMap<&str, Value<'_>>)> = shortcuts
        .iter()
        .map(|(id, props)| (id.as_str(), props))
        .collect();

    let path_str = session_path.as_str();
    let path = match ObjectPath::try_from(path_str) {
        Ok(p) => p,
        Err(_) => return,
    };

    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let result: Result<OwnedObjectPath, zbus::Error> = proxy
        .call("BindShortcuts", &(path, shortcut_refs, "", &options))
        .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "BindShortcuts failed");
    }
}
