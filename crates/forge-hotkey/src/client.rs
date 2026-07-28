use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;
use tokio::sync::{mpsc, oneshot};

use crate::backend::{HotkeyBackend, HotkeyId, NullBackend};
use crate::combo::HotkeyCombo;
use crate::config::HotkeyConfig;
use crate::error::HotkeyError;
use crate::health::{HealthTx, HotkeyHealthSnapshot, make_health_state};
use crate::supervisor::{self, SupervisorCommand};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct EnableFailure {
    pub id: HotkeyId,
    pub combo: HotkeyCombo,
    pub error: HotkeyError,
}

pub struct HotkeyClient {
    pub(crate) id: BuiltinId,
    pub(crate) config: HotkeyConfig,
    pub(crate) backend: Arc<dyn HotkeyBackend>,
    pub(crate) registry: RwLock<HashMap<String, HotkeyId>>,
    pub(crate) id_to_combo: RwLock<HashMap<HotkeyId, HotkeyCombo>>,
    id_counter: AtomicU32,
    pub(crate) publisher: Arc<dyn EventPublisher>,
    pub(crate) health_state: Arc<Mutex<HotkeyHealthSnapshot>>,
    pub(crate) health_tx: HealthTx,
    pub(crate) portal_available: Option<bool>,
    pub(crate) enabled: Arc<AtomicBool>,
    control_tx: mpsc::Sender<SupervisorCommand>,
}

impl HotkeyClient {
    pub(crate) fn start(
        config: HotkeyConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<dyn HotkeyBackend>,
        portal_available: Option<bool>,
    ) -> Arc<Self> {
        let (health_tx, health_state) = make_health_state();
        let (control_tx, control_rx) = mpsc::channel::<SupervisorCommand>(8);

        let client = Arc::new(Self {
            id: BuiltinId::new("hotkey"),
            config,
            backend,
            registry: RwLock::new(HashMap::new()),
            id_to_combo: RwLock::new(HashMap::new()),
            id_counter: AtomicU32::new(1),
            publisher,
            health_state,
            health_tx,
            portal_available,
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
        });

        let fired_rx = client.backend.fired_rx();
        if let Some(rx) = fired_rx {
            let c = Arc::clone(&client);
            tokio::spawn(async move {
                supervisor::run_supervisor(c, rx, control_rx).await;
            });
        }

        client
    }

    pub async fn new(config: HotkeyConfig, publisher: Arc<dyn EventPublisher>) -> Arc<Self> {
        select_and_start(config, publisher).await
    }

    pub async fn register(&self, combo: HotkeyCombo) -> Result<HotkeyId, HotkeyError> {
        let combo_str = combo.as_str().to_owned();

        {
            let guard = self.registry.read().unwrap_or_else(|p| p.into_inner());
            if guard.contains_key(&combo_str) {
                let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
                snap.conflict_count = snap.conflict_count.saturating_add(1);
                return Err(HotkeyError::AlreadyRegistered { combo: combo_str });
            }
        }

        let id = HotkeyId(self.id_counter.fetch_add(1, Ordering::Relaxed));

        if self.os_registration_active() {
            self.backend.register(id, &combo).map_err(|e| {
                if matches!(e, HotkeyError::AlreadyRegistered { .. }) {
                    let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
                    snap.conflict_count = snap.conflict_count.saturating_add(1);
                }
                e
            })?;
        }

        {
            let mut guard = self.registry.write().unwrap_or_else(|p| p.into_inner());
            guard.insert(combo_str.clone(), id);
        }
        {
            let mut guard = self.id_to_combo.write().unwrap_or_else(|p| p.into_inner());
            guard.insert(id, combo);
        }

        supervisor::emit_registered(self, &combo_str, id.0);

        Ok(id)
    }

    pub async fn unregister(&self, id: HotkeyId) -> Result<(), HotkeyError> {
        let combo = {
            let mut guard = self.id_to_combo.write().unwrap_or_else(|p| p.into_inner());
            guard.remove(&id)
        };

        let Some(combo) = combo else {
            return Ok(());
        };

        let combo_str = combo.as_str().to_owned();

        if self.os_registration_active() {
            self.backend.unregister(id)?;
        }

        {
            let mut guard = self.registry.write().unwrap_or_else(|p| p.into_inner());
            guard.remove(&combo_str);
        }

        supervisor::emit_unregistered(self, &combo_str, id.0);

        Ok(())
    }

    pub async fn disable(&self) -> Result<(), HotkeyError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_command(SupervisorCommand::Disable(reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| HotkeyError::SupervisorUnavailable)
    }

    pub async fn enable(&self) -> Result<Vec<EnableFailure>, HotkeyError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_command(SupervisorCommand::Enable(reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| HotkeyError::SupervisorUnavailable)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn os_registration_active(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) || self.backend.delivery_gate_only()
    }

    async fn send_command(&self, cmd: SupervisorCommand) -> Result<(), HotkeyError> {
        tokio::time::timeout(COMMAND_TIMEOUT, self.control_tx.send(cmd))
            .await
            .map_err(|_| HotkeyError::SupervisorUnavailable)?
            .map_err(|_| HotkeyError::SupervisorUnavailable)
    }

    pub fn registered_combos(&self) -> Vec<(HotkeyId, HotkeyCombo)> {
        self.id_to_combo
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(&id, combo)| (id, combo.clone()))
            .collect()
    }

    pub fn portal_available(&self) -> Option<bool> {
        self.portal_available
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(portal_available: Option<bool>) -> Arc<Self> {
        use crate::backend::tests::MockPortalBackend;

        struct NoopPublisher;
        impl EventPublisher for NoopPublisher {
            fn publish(&self, _: forge_events::Event) {}
        }

        let (backend, _tx) = MockPortalBackend::new();
        let (health_tx, health_state) = make_health_state();
        let (control_tx, _control_rx) = mpsc::channel::<SupervisorCommand>(8);

        Arc::new(Self {
            id: BuiltinId::new("hotkey"),
            config: HotkeyConfig::default(),
            backend: Arc::new(backend),
            registry: RwLock::new(HashMap::new()),
            id_to_combo: RwLock::new(HashMap::new()),
            id_counter: AtomicU32::new(1),
            publisher: Arc::new(NoopPublisher),
            health_state,
            health_tx,
            portal_available,
            enabled: Arc::new(AtomicBool::new(true)),
            control_tx,
        })
    }
}

async fn select_and_start(
    config: HotkeyConfig,
    publisher: Arc<dyn EventPublisher>,
) -> Arc<HotkeyClient> {
    #[cfg(target_os = "linux")]
    {
        use crate::backend_evdev::EvdevBackend;
        use crate::backend_portal::PortalBackend;

        let portal_reason = match PortalBackend::try_new(&config.app_name).await {
            Ok(portal) => {
                return HotkeyClient::start(config, publisher, Arc::new(portal), Some(true));
            }
            Err(e) => e.to_string(),
        };

        match EvdevBackend::try_new().await {
            Ok(evdev) => {
                tracing::info!(portal = %portal_reason, "global shortcuts: using evdev backend");
                return HotkeyClient::start(config, publisher, Arc::new(evdev), Some(false));
            }
            Err(e) => {
                tracing::warn!(
                    portal = %portal_reason,
                    evdev = %e,
                    "global shortcuts unavailable: hotkeys will not fire; install forge so the desktop portal grants an app id, or add your user to the 'input' group for evdev access"
                );
                let detail = format!("portal: {portal_reason}; evdev: {e}");
                let client = {
                    let backend = Arc::new(NullBackend::new());
                    HotkeyClient::start(config, Arc::clone(&publisher), backend, None)
                };
                supervisor::emit_portal_unavailable(&client, &detail);
                return client;
            }
        }
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::backend_global::GlobalHotkeyBackend;

        match GlobalHotkeyBackend::new() {
            Ok(backend) => {
                return HotkeyClient::start(config, publisher, Arc::new(backend), None);
            }
            Err(e) => {
                tracing::error!(error = %e, "GlobalHotkeyBackend unavailable");
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let backend = Arc::new(NullBackend::new());
        return HotkeyClient::start(config, publisher, backend, None);
    }

    #[allow(unreachable_code)]
    {
        let backend = Arc::new(NullBackend::new());
        HotkeyClient::start(config, publisher, backend, None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use forge_events::EventPublisher;

    use super::*;
    use crate::backend::tests::{MockFailAllBackend, MockPortalBackend};

    pub(crate) struct RecordingPublisher {
        events: std::sync::Mutex<Vec<forge_events::Event>>,
    }

    impl RecordingPublisher {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self {
                events: std::sync::Mutex::new(Vec::new()),
            })
        }

        pub(crate) fn has_kind(&self, kind: &str) -> bool {
            self.events.lock().unwrap().iter().any(|e| e.kind == kind)
        }

        fn find_kind(&self, kind: &str) -> Option<forge_events::Event> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.kind == kind)
                .cloned()
        }

        fn count_kind(&self, kind: &str) -> usize {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.kind == kind)
                .count()
        }
    }

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: forge_events::Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    pub(crate) fn noop_publisher() -> Arc<dyn EventPublisher> {
        struct Noop;
        impl EventPublisher for Noop {
            fn publish(&self, _: forge_events::Event) {}
        }
        Arc::new(Noop)
    }

    pub(crate) fn start_supervised(
        backend: MockPortalBackend,
        publisher: Arc<dyn EventPublisher>,
    ) -> Arc<HotkeyClient> {
        HotkeyClient::start(
            HotkeyConfig::default(),
            publisher,
            Arc::new(backend),
            Some(true),
        )
    }

    async fn drain_fired(tx: &mpsc::Sender<crate::backend::HotkeyFiredEvent>) {
        while tx.capacity() < tx.max_capacity() {
            tokio::task::yield_now().await;
        }
    }

    fn combo(s: &str) -> HotkeyCombo {
        HotkeyCombo::parse(s).unwrap()
    }

    #[tokio::test]
    async fn register_returns_id_and_stores_combo() {
        let client = HotkeyClient::new_for_test(Some(true));
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();
        let combos = client.registered_combos();
        assert!(combos.iter().any(|(i, stored)| i == &id && stored == &c));
    }

    #[tokio::test]
    async fn register_same_combo_twice_returns_already_registered() {
        let client = HotkeyClient::new_for_test(Some(true));
        let c = combo("Ctrl+A");
        client.register(c.clone()).await.unwrap();
        let err = client.register(c).await.unwrap_err();
        assert!(matches!(err, HotkeyError::AlreadyRegistered { .. }));
    }

    #[tokio::test]
    async fn register_emits_hotkey_registered_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        client.register(combo("Ctrl+B")).await.unwrap();
        assert!(publisher.has_kind("hotkey.registered"));
    }

    #[tokio::test]
    async fn unregister_removes_combo_and_emits_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        let id = client.register(combo("Ctrl+C")).await.unwrap();
        client.unregister(id).await.unwrap();
        assert!(client.registered_combos().is_empty());
        assert!(publisher.has_kind("hotkey.unregistered"));
    }

    #[tokio::test]
    async fn backend_conflict_returns_already_registered() {
        let (backend, _tx) = MockPortalBackend::new();
        backend.add_conflict("Alt+X");
        let client = start_supervised(backend, noop_publisher());
        let err = client.register(combo("Alt+X")).await.unwrap_err();
        assert!(matches!(err, HotkeyError::AlreadyRegistered { .. }));
    }

    #[tokio::test]
    async fn fail_all_backend_register_returns_permission_denied() {
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            noop_publisher(),
            Arc::new(MockFailAllBackend::new()),
            None,
        );
        let err = client.register(combo("Ctrl+D")).await.unwrap_err();
        assert!(matches!(err, HotkeyError::PermissionDenied));
    }

    #[tokio::test]
    async fn triggered_event_emitted_on_fired_rx() {
        let publisher = RecordingPublisher::new();
        let (backend, inject_tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();

        inject_tx
            .send(crate::backend::HotkeyFiredEvent {
                id,
                combo: c,
                timestamp_us: 0,
            })
            .await
            .unwrap();
        drain_fired(&inject_tx).await;

        let ev = publisher.find_kind("hotkey.global.pressed").unwrap();
        assert_eq!(ev.payload["combo"], "Ctrl+F1");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn portal_unavailable_event_carries_reason_token_and_human_detail() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);

        crate::supervisor::emit_portal_unavailable(&client, "portal: no session; evdev: denied");

        let ev = publisher.find_kind("hotkey.portal.unavailable").unwrap();
        assert_eq!(ev.payload["reason"], "no_hotkey_backend_available");
        assert_eq!(ev.payload["detail"], "portal: no session; evdev: denied");
    }

    #[tokio::test]
    async fn disable_drops_os_registrations_but_keeps_the_client_registry() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();

        client.disable().await.unwrap();

        assert!(os_registered.lock().unwrap().is_empty());
        assert_eq!(client.registered_combos(), vec![(id, c)]);
    }

    #[tokio::test]
    async fn enable_restores_os_registrations_for_the_same_ids() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        let first = client.register(combo("Ctrl+F1")).await.unwrap();
        let second = client.register(combo("Alt+F2")).await.unwrap();

        client.disable().await.unwrap();
        let failures = client.enable().await.unwrap();

        assert!(failures.is_empty());
        let os = os_registered.lock().unwrap();
        assert_eq!(os.get(&first.0).map(String::as_str), Some("Ctrl+F1"));
        assert_eq!(os.get(&second.0).map(String::as_str), Some("Alt+F2"));
    }

    #[tokio::test]
    async fn combo_registered_while_disabled_reaches_the_os_only_after_enable() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());

        client.disable().await.unwrap();
        let id = client.register(combo("Ctrl+F3")).await.unwrap();
        assert!(os_registered.lock().unwrap().is_empty());

        client.enable().await.unwrap();

        assert_eq!(
            os_registered.lock().unwrap().get(&id.0).map(String::as_str),
            Some("Ctrl+F3")
        );
    }

    #[tokio::test]
    async fn enable_reports_the_refused_combo_and_still_registers_the_others() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let refuse = Arc::clone(&backend.fail_on);
        let client = start_supervised(backend, noop_publisher());
        let taken = client.register(combo("Alt+X")).await.unwrap();
        let free = client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();
        refuse.lock().unwrap().insert("Alt+X".to_owned());
        let failures = client.enable().await.unwrap();

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].id, taken);
        assert_eq!(failures[0].combo, combo("Alt+X"));
        assert!(matches!(
            failures[0].error,
            HotkeyError::AlreadyRegistered { .. }
        ));
        assert!(os_registered.lock().unwrap().contains_key(&free.0));
    }

    #[tokio::test]
    async fn a_delivery_gate_only_backend_keeps_its_os_grabs_across_a_disable_cycle() {
        let (backend, _tx) = MockPortalBackend::new_delivery_gate_only();
        let os_registered = Arc::clone(&backend.registered);
        let registers = Arc::clone(&backend.register_calls);
        let unregisters = Arc::clone(&backend.unregister_calls);
        let client = start_supervised(backend, noop_publisher());
        let id = client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();
        client.enable().await.unwrap();

        assert_eq!(registers.load(Ordering::Relaxed), 1);
        assert_eq!(unregisters.load(Ordering::Relaxed), 0);
        assert!(os_registered.lock().unwrap().contains_key(&id.0));
    }

    #[tokio::test]
    async fn a_fired_event_is_dropped_while_the_engine_is_disabled() {
        let publisher = RecordingPublisher::new();
        let (backend, inject_tx) = MockPortalBackend::new_delivery_gate_only();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();

        client.disable().await.unwrap();
        inject_tx
            .send(crate::backend::HotkeyFiredEvent {
                id,
                combo: c,
                timestamp_us: 0,
            })
            .await
            .unwrap();
        drain_fired(&inject_tx).await;

        assert!(!publisher.has_kind("hotkey.global.pressed"));
    }

    #[tokio::test]
    async fn a_fired_event_is_delivered_again_after_enable() {
        let publisher = RecordingPublisher::new();
        let (backend, inject_tx) = MockPortalBackend::new_delivery_gate_only();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();

        client.disable().await.unwrap();
        client.enable().await.unwrap();
        inject_tx
            .send(crate::backend::HotkeyFiredEvent {
                id,
                combo: c,
                timestamp_us: 0,
            })
            .await
            .unwrap();
        drain_fired(&inject_tx).await;

        assert!(publisher.has_kind("hotkey.global.pressed"));
    }

    #[tokio::test]
    async fn engine_state_events_follow_the_transition_direction() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);

        client.disable().await.unwrap();
        assert!(publisher.has_kind("hotkey.engine.disabled"));
        assert!(!publisher.has_kind("hotkey.engine.enabled"));

        client.enable().await.unwrap();
        assert!(publisher.has_kind("hotkey.engine.enabled"));
    }

    #[tokio::test]
    async fn a_redundant_disable_leaves_the_following_enable_intact() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        let id = client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();
        client.disable().await.unwrap();
        let failures = client.enable().await.unwrap();

        assert!(failures.is_empty());
        assert!(client.is_enabled());
        assert!(os_registered.lock().unwrap().contains_key(&id.0));
    }

    #[tokio::test]
    async fn disable_has_already_taken_effect_when_it_returns() {
        let (backend, _tx) = MockPortalBackend::new();
        let os_registered = Arc::clone(&backend.registered);
        let client = start_supervised(backend, noop_publisher());
        client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();

        assert!(!client.is_enabled());
        assert!(os_registered.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_no_op_transition_publishes_no_second_engine_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);

        client.disable().await.unwrap();
        client.disable().await.unwrap();
        client.enable().await.unwrap();
        client.enable().await.unwrap();

        assert_eq!(publisher.count_kind("hotkey.engine.disabled"), 1);
        assert_eq!(publisher.count_kind("hotkey.engine.enabled"), 1);
    }

    #[tokio::test]
    async fn a_no_op_enable_leaves_the_backend_untouched() {
        let (backend, _tx) = MockPortalBackend::new();
        let registers = Arc::clone(&backend.register_calls);
        let client = start_supervised(backend, noop_publisher());
        client.register(combo("Ctrl+F1")).await.unwrap();

        let failures = client.enable().await.unwrap();

        assert!(failures.is_empty());
        assert_eq!(registers.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn enable_failed_event_lists_every_refused_combo() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let refuse = Arc::clone(&backend.fail_on);
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        client.register(combo("Alt+X")).await.unwrap();
        client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();
        refuse.lock().unwrap().insert("Alt+X".to_owned());
        client.enable().await.unwrap();

        let ev = publisher.find_kind("hotkey.engine.enable_failed").unwrap();
        assert_eq!(ev.payload["combos"], serde_json::json!(["Alt+X"]));
    }

    #[tokio::test]
    async fn a_clean_enable_publishes_no_enable_failed_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        client.register(combo("Ctrl+F1")).await.unwrap();

        client.disable().await.unwrap();
        client.enable().await.unwrap();

        assert!(!publisher.has_kind("hotkey.engine.enable_failed"));
    }

    #[tokio::test]
    async fn fired_events_queued_before_a_disable_are_all_still_delivered() {
        let publisher = RecordingPublisher::new();
        let (backend, inject_tx) = MockPortalBackend::new();
        let client = start_supervised(backend, Arc::clone(&publisher) as Arc<dyn EventPublisher>);
        let c = combo("Ctrl+F1");
        let id = client.register(c.clone()).await.unwrap();

        for _ in 0..5 {
            inject_tx
                .send(crate::backend::HotkeyFiredEvent {
                    id,
                    combo: c.clone(),
                    timestamp_us: 0,
                })
                .await
                .unwrap();
        }
        client.disable().await.unwrap();

        assert_eq!(publisher.count_kind("hotkey.global.pressed"), 5);
    }

    #[tokio::test]
    async fn commands_report_supervisor_unavailable_when_no_supervisor_runs() {
        let client = HotkeyClient::new_for_test(Some(true));

        assert!(matches!(
            client.disable().await,
            Err(HotkeyError::SupervisorUnavailable)
        ));
        assert!(matches!(
            client.enable().await,
            Err(HotkeyError::SupervisorUnavailable)
        ));
        assert!(client.is_enabled());
    }
}
