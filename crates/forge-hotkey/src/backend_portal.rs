#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::Mutex;

use tokio::sync::mpsc;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::backend::{HotkeyBackend, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;

pub(crate) struct PortalBackend {
    cmd_tx: mpsc::Sender<PortalCmd>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
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

        let session_path = create_portal_session(&conn, app_name).await.map_err(|e| {
            HotkeyError::PortalUnavailable {
                reason: format!("GlobalShortcuts portal not available: {e}"),
            }
        })?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<PortalCmd>(64);
        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(64);

        tokio::spawn(run_portal_task(
            conn,
            session_path,
            app_name.to_owned(),
            cmd_rx,
            fired_tx,
        ));

        Ok(Self {
            cmd_tx,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
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
}

async fn create_portal_session(conn: &Connection, app_name: &str) -> zbus::Result<OwnedObjectPath> {
    let token = make_token(app_name, "session");

    let proxy = zbus::Proxy::new(
        conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.GlobalShortcuts",
    )
    .await?;

    let options: HashMap<&str, Value<'_>> = [
        ("handle_token", Value::Str(token.as_str().into())),
        ("session_handle_token", Value::Str(token.as_str().into())),
    ]
    .into_iter()
    .collect();

    let (response, results): (u32, HashMap<String, OwnedValue>) =
        proxy.call("CreateSession", &(options,)).await?;

    if response != 0 {
        return Err(zbus::Error::Failure(format!(
            "CreateSession returned non-zero response: {response}"
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
) {
    let mut registered: HashMap<HotkeyId, HotkeyCombo> = HashMap::new();
    let mut combo_to_id: HashMap<String, HotkeyId> = HashMap::new();

    let (activated_tx, mut activated_rx) = mpsc::channel::<(String, u64)>(64);
    let (restart_tx, mut restart_rx) = mpsc::channel::<()>(4);

    let conn_clone = conn.clone();
    let session_path_clone = session_path.clone();
    let app_name_clone = app_name.clone();
    let activated_tx_clone = activated_tx.clone();
    let restart_tx_clone = restart_tx.clone();

    tokio::spawn(signal_listener_task(
        conn_clone,
        session_path_clone,
        app_name_clone,
        activated_tx_clone,
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
            Some((shortcut_id, timestamp)) = activated_rx.recv() => {
                if let Some(&id) = combo_to_id.get(&shortcut_id)
                    && let Ok(combo) = HotkeyCombo::parse(&shortcut_id)
                {
                    let _ = fired_tx.send(HotkeyFiredEvent {
                        id,
                        combo,
                        timestamp_us: timestamp,
                    }).await;
                }
            }
            Some(()) = restart_rx.recv() => {
                tracing::info!("portal daemon restarted - recreating session");
                let conn_clone = conn.clone();
                let app_name_clone = app_name.clone();
                let activated_tx_clone = activated_tx.clone();
                let restart_tx_clone = restart_tx.clone();
                tokio::spawn(signal_listener_task(
                    conn_clone,
                    session_path.clone(),
                    app_name_clone,
                    activated_tx_clone,
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
    activated_tx: mpsc::Sender<(String, u64)>,
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

    let name_owner_stream = match dbus_proxy.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to NameOwnerChanged");
            return;
        }
    };

    let mut activated_stream = std::pin::pin!(activated_stream);
    let mut name_owner_stream = std::pin::pin!(name_owner_stream);

    loop {
        tokio::select! {
            maybe_msg = TokioStreamExt::next(&mut activated_stream) => {
                let Some(msg) = maybe_msg else { break };
                let body: Result<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>), _> =
                    msg.body().deserialize();
                if let Ok((_session, shortcut_id, timestamp, _opts)) = body {
                    let _ = activated_tx.send((shortcut_id, timestamp)).await;
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
    let result: Result<(u32, HashMap<String, OwnedValue>), zbus::Error> = proxy
        .call("BindShortcuts", &(path, shortcut_refs, "", &options))
        .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "BindShortcuts failed");
    }
}
