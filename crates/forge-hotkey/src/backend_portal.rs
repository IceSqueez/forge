#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;
use zbus::Connection;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::backend::{BackendSessionEvent, HotkeyBackend, HotkeyEdge, HotkeyFiredEvent, HotkeyId};
use crate::combo::HotkeyCombo;
use crate::error::HotkeyError;
use crate::portal_request::{
    PORTAL_DESTINATION, PORTAL_OBJECT_PATH, PortalOutcome, call_with_response, make_token,
    object_path_result,
};
use crate::portal_trigger::preferred_trigger;

const GLOBAL_SHORTCUTS_INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const HOST_REGISTRY_INTERFACE: &str = "org.freedesktop.host.portal.Registry";
const SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";
const ACTIVATED_SIGNAL: &str = "Activated";
const DEACTIVATED_SIGNAL: &str = "Deactivated";

const OPTION_HANDLE_TOKEN: &str = "handle_token";
const OPTION_SESSION_HANDLE_TOKEN: &str = "session_handle_token";
const RESULT_SESSION_HANDLE: &str = "session_handle";
const RESULT_SHORTCUTS: &str = "shortcuts";
const SHORTCUT_DESCRIPTION: &str = "description";
const SHORTCUT_PREFERRED_TRIGGER: &str = "preferred_trigger";
const SHORTCUT_TRIGGER_DESCRIPTION: &str = "trigger_description";
const NO_PARENT_WINDOW: &str = "";

const CREATE_SESSION_TIMEOUT: Duration = Duration::from_secs(10);
const BIND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const SESSION_RECREATE_ATTEMPTS: u32 = 3;
const SESSION_RECREATE_FIRST_BACKOFF: Duration = Duration::from_secs(1);

const COMMAND_QUEUE_CAPACITY: usize = 64;
const FIRED_QUEUE_CAPACITY: usize = 64;
const EDGE_QUEUE_CAPACITY: usize = 64;
const NOTICE_QUEUE_CAPACITY: usize = 4;

pub(crate) struct PortalBackend {
    cmd_tx: mpsc::Sender<PortalCmd>,
    fired_rx_slot: Mutex<Option<mpsc::Receiver<HotkeyFiredEvent>>>,
    restart_rx_slot: Mutex<Option<mpsc::Receiver<()>>>,
    session_rx_slot: Mutex<Option<mpsc::Receiver<BackendSessionEvent>>>,
}

enum PortalCmd {
    Register(
        HotkeyId,
        HotkeyCombo,
        oneshot::Sender<Result<(), HotkeyError>>,
    ),
    Unregister(HotkeyId),
}

struct ShortcutEdge {
    shortcut_id: String,
    edge: HotkeyEdge,
}

struct PortalOutputs {
    fired_tx: mpsc::Sender<HotkeyFiredEvent>,
    restart_notice_tx: mpsc::Sender<()>,
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

        let session_path = create_portal_session(&conn, app_name)
            .await
            .map_err(|reason| HotkeyError::PortalUnavailable {
                reason: format!("GlobalShortcuts portal not available: {reason}"),
            })?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<PortalCmd>(COMMAND_QUEUE_CAPACITY);
        let (fired_tx, fired_rx) = mpsc::channel::<HotkeyFiredEvent>(FIRED_QUEUE_CAPACITY);
        let (restart_notice_tx, restart_notice_rx) = mpsc::channel::<()>(NOTICE_QUEUE_CAPACITY);
        let (session_tx, session_rx) = mpsc::channel::<BackendSessionEvent>(NOTICE_QUEUE_CAPACITY);

        let session = PortalSession {
            conn,
            app_name: app_name.to_owned(),
            session_path,
            session_bound: false,
            registered: HashMap::new(),
            shortcut_ids: HashMap::new(),
            session_tx,
            session_lost: false,
        };
        let outputs = PortalOutputs {
            fired_tx,
            restart_notice_tx,
        };
        tokio::spawn(session.run(cmd_rx, outputs));

        Ok(Self {
            cmd_tx,
            fired_rx_slot: Mutex::new(Some(fired_rx)),
            restart_rx_slot: Mutex::new(Some(restart_notice_rx)),
            session_rx_slot: Mutex::new(Some(session_rx)),
        })
    }
}

#[async_trait::async_trait]
impl HotkeyBackend for PortalBackend {
    async fn register(&self, id: HotkeyId, combo: &HotkeyCombo) -> Result<(), HotkeyError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(PortalCmd::Register(id, combo.clone(), reply_tx))
            .await
            .map_err(|_| HotkeyError::Backend("portal session task stopped".to_owned()))?;
        reply_rx
            .await
            .map_err(|_| HotkeyError::Backend("portal session task stopped".to_owned()))?
    }

    async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
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

    fn session_events_rx(&self) -> Option<mpsc::Receiver<BackendSessionEvent>> {
        self.session_rx_slot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
    }
}

struct PortalSession {
    conn: Connection,
    app_name: String,
    session_path: OwnedObjectPath,
    session_bound: bool,
    registered: HashMap<HotkeyId, HotkeyCombo>,
    shortcut_ids: HashMap<String, HotkeyId>,
    session_tx: mpsc::Sender<BackendSessionEvent>,
    session_lost: bool,
}

impl PortalSession {
    async fn run(mut self, mut cmd_rx: mpsc::Receiver<PortalCmd>, outputs: PortalOutputs) {
        let (edge_tx, mut edge_rx) = mpsc::channel::<ShortcutEdge>(EDGE_QUEUE_CAPACITY);
        let (owner_restart_tx, mut owner_restart_rx) = mpsc::channel::<()>(NOTICE_QUEUE_CAPACITY);

        tokio::spawn(signal_listener_task(
            self.conn.clone(),
            edge_tx.clone(),
            owner_restart_tx.clone(),
        ));

        loop {
            tokio::select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(PortalCmd::Register(id, combo, reply)) => {
                            let outcome = self.register(id, combo).await;
                            let _ = reply.send(outcome);
                        }
                        Some(PortalCmd::Unregister(id)) => self.unregister(id).await,
                        None => break,
                    }
                }
                Some(shortcut) = edge_rx.recv() => {
                    self.forward_edge(shortcut, &outputs.fired_tx).await;
                }
                Some(()) = owner_restart_rx.recv() => {
                    tracing::info!("portal daemon restarted - recreating the shortcuts session");
                    let _ = outputs.restart_notice_tx.send(()).await;
                    tokio::spawn(signal_listener_task(
                        self.conn.clone(),
                        edge_tx.clone(),
                        owner_restart_tx.clone(),
                    ));
                    let outcome = self.recreate().await;
                    self.note_session_outcome(outcome).await;
                }
            }
        }
    }

    async fn register(&mut self, id: HotkeyId, combo: HotkeyCombo) -> Result<(), HotkeyError> {
        let shortcut_id = combo.as_str().to_owned();
        self.shortcut_ids.insert(shortcut_id.clone(), id);
        self.registered.insert(id, combo);

        let rejection = match self.bind().await {
            Ok(bound) if bound.contains_key(&shortcut_id) => {
                self.note_session_outcome(Ok(())).await;
                return Ok(());
            }
            Ok(_) => {
                self.registered.remove(&id);
                self.shortcut_ids.remove(&shortcut_id);
                self.note_session_outcome(Ok(())).await;
                "the desktop left it out of the bound shortcuts".to_owned()
            }
            Err(reason) => {
                self.registered.remove(&id);
                self.shortcut_ids.remove(&shortcut_id);
                let restored = self.bind().await.map(|_| ());
                self.note_session_outcome(restored).await;
                reason
            }
        };

        Err(HotkeyError::BindRejected {
            combo: shortcut_id,
            reason: rejection,
        })
    }

    async fn unregister(&mut self, id: HotkeyId) {
        let Some(combo) = self.registered.remove(&id) else {
            return;
        };
        self.shortcut_ids.remove(combo.as_str());
        let outcome = self.bind().await.map(|_| ());
        self.note_session_outcome(outcome).await;
    }

    async fn note_session_outcome(&mut self, outcome: Result<(), String>) {
        let event = match outcome {
            Ok(()) if self.session_lost => BackendSessionEvent::Restored,
            Ok(()) => return,
            Err(reason) => {
                tracing::warn!(%reason, "global shortcuts session is not bound");
                BackendSessionEvent::Lost(reason)
            }
        };
        self.session_lost = matches!(event, BackendSessionEvent::Lost(_));
        let _ = self.session_tx.send(event).await;
    }

    async fn forward_edge(
        &self,
        shortcut: ShortcutEdge,
        fired_tx: &mpsc::Sender<HotkeyFiredEvent>,
    ) {
        let Some(&id) = self.shortcut_ids.get(&shortcut.shortcut_id) else {
            return;
        };
        let Some(combo) = self.registered.get(&id) else {
            return;
        };
        let _ = fired_tx
            .send(HotkeyFiredEvent {
                id,
                combo: combo.clone(),
                timestamp_us: wall_clock_us(),
                edge: shortcut.edge,
            })
            .await;
    }

    async fn recreate(&mut self) -> Result<(), String> {
        register_host_app_id(&self.conn, &self.app_name).await;
        self.session_bound = false;

        let mut backoff = SESSION_RECREATE_FIRST_BACKOFF;
        let mut attempt = 1;
        self.session_path = loop {
            match create_portal_session(&self.conn, &self.app_name).await {
                Ok(path) => break path,
                Err(reason) if attempt >= SESSION_RECREATE_ATTEMPTS => {
                    return Err(format!("CreateSession failed: {reason}"));
                }
                Err(reason) => {
                    tracing::debug!(attempt, %reason, "CreateSession after portal restart failed; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.saturating_mul(2);
                    attempt += 1;
                }
            }
        };

        if self.registered.is_empty() {
            return Ok(());
        }
        let bound = self.bind().await?;
        for combo in self.registered.values() {
            if !bound.contains_key(combo.as_str()) {
                tracing::warn!(combo = %combo, "the desktop did not rebind this shortcut after the portal restart");
            }
        }
        Ok(())
    }

    async fn ensure_unbound_session(&mut self) -> Result<(), String> {
        if !self.session_bound {
            return Ok(());
        }
        close_session(&self.conn, &self.session_path).await;
        self.session_path = create_portal_session(&self.conn, &self.app_name).await?;
        self.session_bound = false;
        Ok(())
    }

    async fn bind(&mut self) -> Result<HashMap<String, String>, String> {
        self.ensure_unbound_session().await?;
        self.session_bound = true;

        let proxy = global_shortcuts_proxy(&self.conn)
            .await
            .map_err(|e| e.to_string())?;
        let session =
            ObjectPath::try_from(self.session_path.as_str()).map_err(|e| e.to_string())?;

        let shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)> = self
            .registered
            .values()
            .map(|combo| (combo.as_str(), shortcut_properties(combo)))
            .collect();

        let handle_token = make_token(&self.app_name, "bind");
        let options: HashMap<&str, Value<'_>> =
            HashMap::from([(OPTION_HANDLE_TOKEN, Value::from(handle_token.as_str()))]);

        let results = call_with_response(
            &self.conn,
            &proxy,
            "BindShortcuts",
            &(session, shortcuts, NO_PARENT_WINDOW, options),
            &handle_token,
            BIND_RESPONSE_TIMEOUT,
        )
        .await
        .map_err(|e| e.to_string())?
        .into_results("BindShortcuts")?;

        let bound = bound_triggers(&results);
        for (shortcut_id, trigger) in &bound {
            if trigger.is_empty() {
                tracing::warn!(
                    shortcut = %shortcut_id,
                    "global shortcut bound without a key; assign one in the desktop shortcut settings"
                );
            } else {
                tracing::info!(shortcut = %shortcut_id, %trigger, "global shortcut bound");
            }
        }
        Ok(bound)
    }
}

fn shortcut_properties(combo: &HotkeyCombo) -> HashMap<&'static str, Value<'_>> {
    let mut props: HashMap<&'static str, Value<'_>> =
        HashMap::from([(SHORTCUT_DESCRIPTION, Value::from(combo.as_str()))]);
    if let Some(trigger) = preferred_trigger(combo) {
        props.insert(SHORTCUT_PREFERRED_TRIGGER, Value::from(trigger));
    }
    props
}

fn bound_triggers(results: &HashMap<String, OwnedValue>) -> HashMap<String, String> {
    let Some(Value::Array(entries)) = results.get(RESULT_SHORTCUTS).map(|v| &**v) else {
        return HashMap::new();
    };
    entries
        .inner()
        .iter()
        .filter_map(|entry| {
            let Value::Structure(shortcut) = entry else {
                return None;
            };
            let [Value::Str(id), Value::Dict(props)] = shortcut.fields() else {
                return None;
            };
            let trigger = props
                .iter()
                .find_map(|(key, value)| match (key, unwrap_variant(value)) {
                    (Value::Str(k), Value::Str(trigger))
                        if k.as_str() == SHORTCUT_TRIGGER_DESCRIPTION =>
                    {
                        Some(trigger.as_str().to_owned())
                    }
                    _ => None,
                })
                .unwrap_or_default();
            Some((id.as_str().to_owned(), trigger))
        })
        .collect()
}

fn unwrap_variant<'a>(value: &'a Value<'a>) -> &'a Value<'a> {
    match value {
        Value::Value(inner) => inner,
        other => other,
    }
}

async fn global_shortcuts_proxy(conn: &Connection) -> zbus::Result<zbus::Proxy<'static>> {
    zbus::Proxy::new(
        conn,
        PORTAL_DESTINATION,
        PORTAL_OBJECT_PATH,
        GLOBAL_SHORTCUTS_INTERFACE,
    )
    .await
}

async fn register_host_app_id(conn: &Connection, app_id: &str) {
    let proxy = match zbus::Proxy::new(
        conn,
        PORTAL_DESTINATION,
        PORTAL_OBJECT_PATH,
        HOST_REGISTRY_INTERFACE,
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

async fn create_portal_session(
    conn: &Connection,
    app_name: &str,
) -> Result<OwnedObjectPath, String> {
    let proxy = global_shortcuts_proxy(conn)
        .await
        .map_err(|e| e.to_string())?;

    let handle_token = make_token(app_name, "session");
    let session_token = make_token(app_name, "sess");
    let options: HashMap<&str, Value<'_>> = HashMap::from([
        (OPTION_HANDLE_TOKEN, Value::from(handle_token.as_str())),
        (
            OPTION_SESSION_HANDLE_TOKEN,
            Value::from(session_token.as_str()),
        ),
    ]);

    let outcome: PortalOutcome = call_with_response(
        conn,
        &proxy,
        "CreateSession",
        &(options,),
        &handle_token,
        CREATE_SESSION_TIMEOUT,
    )
    .await
    .map_err(|e| e.to_string())?;
    let results = outcome.into_results("CreateSession")?;

    object_path_result(&results, RESULT_SESSION_HANDLE)
        .ok_or_else(|| "CreateSession returned no usable session_handle".to_owned())
}

async fn close_session(conn: &Connection, session_path: &OwnedObjectPath) {
    let proxy = match zbus::Proxy::new(
        conn,
        PORTAL_DESTINATION,
        session_path.as_str(),
        SESSION_INTERFACE,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "portal session proxy unavailable; skipping Close");
            return;
        }
    };
    if let Err(e) = proxy.call::<_, _, ()>("Close", &()).await {
        tracing::debug!(error = %e, "portal session Close failed");
    }
}

fn wall_clock_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

async fn signal_listener_task(
    conn: Connection,
    edge_tx: mpsc::Sender<ShortcutEdge>,
    owner_restart_tx: mpsc::Sender<()>,
) {
    let shortcuts_proxy = match global_shortcuts_proxy(&conn).await {
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

    let activated_stream = match shortcuts_proxy.receive_signal(ACTIVATED_SIGNAL).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to subscribe to Activated signal");
            return;
        }
    };

    let deactivated_stream = match shortcuts_proxy.receive_signal(DEACTIVATED_SIGNAL).await {
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
            maybe_msg = activated_stream.next() => {
                let Some(msg) = maybe_msg else { break };
                if let Some(shortcut) = parse_shortcut_signal(&msg, HotkeyEdge::Press) {
                    let _ = edge_tx.send(shortcut).await;
                }
            }
            maybe_msg = deactivated_stream.next() => {
                let Some(msg) = maybe_msg else { break };
                if let Some(shortcut) = parse_shortcut_signal(&msg, HotkeyEdge::Release) {
                    let _ = edge_tx.send(shortcut).await;
                }
            }
            maybe_change = name_owner_stream.next() => {
                let Some(change) = maybe_change else { break };
                if let Ok(args) = change.args()
                    && args.name() == PORTAL_DESTINATION
                    && !args.new_owner().as_deref().unwrap_or("").is_empty()
                {
                    let _ = owner_restart_tx.send(()).await;
                    return;
                }
            }
        }
    }
}

fn parse_shortcut_signal(msg: &zbus::Message, edge: HotkeyEdge) -> Option<ShortcutEdge> {
    let (_session, shortcut_id, _timestamp, _options): (
        OwnedObjectPath,
        String,
        u64,
        HashMap<String, OwnedValue>,
    ) = msg.body().deserialize().ok()?;
    Some(ShortcutEdge { shortcut_id, edge })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SESSION: &str = "/org/freedesktop/portal/desktop/session/1_42/forge_sess_0";

    fn message<B>(interface: &str, member: &str, body: &B) -> zbus::Message
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        zbus::Message::signal(PORTAL_OBJECT_PATH, interface, member)
            .unwrap()
            .build(body)
            .unwrap()
    }

    fn bind_response(
        shortcuts: Vec<(&str, HashMap<&str, Value<'_>>)>,
    ) -> HashMap<String, OwnedValue> {
        let mut results: HashMap<&str, Value<'_>> = HashMap::new();
        results.insert(RESULT_SHORTCUTS, Value::from(shortcuts));
        let msg = message(
            "org.freedesktop.portal.Request",
            "Response",
            &(0_u32, results),
        );
        let (_code, decoded): (u32, HashMap<String, OwnedValue>) =
            msg.body().deserialize().unwrap();
        decoded
    }

    fn unchecked_combo(raw: &str) -> HotkeyCombo {
        serde_json::from_value(serde_json::Value::String(raw.to_owned())).unwrap()
    }

    #[test]
    fn bound_triggers_reads_each_returned_shortcut_with_its_trigger_description() {
        let results = bind_response(vec![
            (
                "Ctrl+F1",
                HashMap::from([
                    (SHORTCUT_DESCRIPTION, Value::from("Ctrl+F1")),
                    (SHORTCUT_TRIGGER_DESCRIPTION, Value::from("Ctrl + F1")),
                ]),
            ),
            (
                "Alt+X",
                HashMap::from([(SHORTCUT_DESCRIPTION, Value::from("Alt+X"))]),
            ),
        ]);

        let bound = bound_triggers(&results);

        assert_eq!(
            bound,
            HashMap::from([
                ("Ctrl+F1".to_owned(), "Ctrl + F1".to_owned()),
                ("Alt+X".to_owned(), String::new()),
            ])
        );
    }

    #[test]
    fn bound_triggers_is_empty_for_an_empty_subset_or_a_missing_shortcuts_key() {
        let empty_subset = bind_response(Vec::new());
        let missing_key: HashMap<String, OwnedValue> = HashMap::new();

        for results in [empty_subset, missing_key] {
            assert!(bound_triggers(&results).is_empty());
        }
    }

    #[test]
    fn shortcut_properties_omit_the_trigger_for_a_combo_the_spec_cannot_express() {
        let combo = unchecked_combo("Ctrl+PrintScreen");

        let props = shortcut_properties(&combo);

        assert!(props.contains_key(SHORTCUT_DESCRIPTION));
        assert!(!props.contains_key(SHORTCUT_PREFERRED_TRIGGER));
    }

    #[test]
    fn shortcut_properties_carry_the_preferred_trigger_for_a_supported_combo() {
        let combo = HotkeyCombo::parse("Ctrl+F1").unwrap();

        let props = shortcut_properties(&combo);

        assert_eq!(
            props.get(SHORTCUT_PREFERRED_TRIGGER),
            Some(&Value::from("CTRL+F1"))
        );
    }

    #[test]
    fn activated_and_deactivated_signals_map_to_press_and_release_of_the_shortcut() {
        let session = ObjectPath::try_from(SESSION).unwrap();
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        for (member, edge) in [
            (ACTIVATED_SIGNAL, HotkeyEdge::Press),
            (DEACTIVATED_SIGNAL, HotkeyEdge::Release),
        ] {
            let msg = message(
                GLOBAL_SHORTCUTS_INTERFACE,
                member,
                &(session.clone(), "Ctrl+F1", 123_456_u64, options.clone()),
            );

            let parsed = parse_shortcut_signal(&msg, edge).unwrap();

            assert_eq!(
                (parsed.shortcut_id.as_str(), parsed.edge),
                ("Ctrl+F1", edge)
            );
        }
    }

    #[test]
    fn a_shortcut_signal_with_an_unexpected_body_is_ignored() {
        let msg = message(GLOBAL_SHORTCUTS_INTERFACE, ACTIVATED_SIGNAL, &("Ctrl+F1",));

        assert!(parse_shortcut_signal(&msg, HotkeyEdge::Press).is_none());
    }
}
