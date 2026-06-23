use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;

use crate::backend::{HotkeyBackend, HotkeyId, NullBackend};
use crate::combo::HotkeyCombo;
use crate::config::HotkeyConfig;
use crate::error::HotkeyError;
use crate::health::{HealthTx, HotkeyHealthSnapshot, make_health_state};
use crate::supervisor;

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
}

impl HotkeyClient {
    pub(crate) fn start(
        config: HotkeyConfig,
        publisher: Arc<dyn EventPublisher>,
        backend: Arc<dyn HotkeyBackend>,
        portal_available: Option<bool>,
    ) -> Arc<Self> {
        let (health_tx, health_state) = make_health_state();

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
        });

        let fired_rx = client.backend.fired_rx();
        if let Some(rx) = fired_rx {
            let c = Arc::clone(&client);
            tokio::spawn(async move {
                supervisor::run_supervisor(c, rx).await;
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

        self.backend.register(id, &combo).map_err(|e| {
            if matches!(e, HotkeyError::AlreadyRegistered { .. }) {
                let mut snap = self.health_state.lock().unwrap_or_else(|p| p.into_inner());
                snap.conflict_count = snap.conflict_count.saturating_add(1);
            }
            e
        })?;

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

        self.backend.unregister(id)?;

        {
            let mut guard = self.registry.write().unwrap_or_else(|p| p.into_inner());
            guard.remove(&combo_str);
        }

        supervisor::emit_unregistered(self, &combo_str, id.0);

        Ok(())
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

        match PortalBackend::try_new(&config.app_name).await {
            Ok(portal) => {
                return HotkeyClient::start(config, publisher, Arc::new(portal), Some(true));
            }
            Err(e) => {
                tracing::info!(reason = %e, "portal unavailable, trying evdev");
            }
        }

        match EvdevBackend::try_new().await {
            Ok(evdev) => {
                return HotkeyClient::start(config, publisher, Arc::new(evdev), Some(false));
            }
            Err(e) => {
                tracing::warn!(error = %e, "evdev unavailable");
                let client = {
                    let backend = Arc::new(NullBackend::new());
                    HotkeyClient::start(config, Arc::clone(&publisher), backend, None)
                };
                supervisor::emit_portal_unavailable(&client, &e.to_string());
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
mod tests {
    use forge_events::EventPublisher;

    use super::*;
    use crate::backend::tests::{MockFailAllBackend, MockPortalBackend};

    struct RecordingPublisher {
        events: std::sync::Mutex<Vec<forge_events::Event>>,
    }

    impl RecordingPublisher {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                events: std::sync::Mutex::new(Vec::new()),
            })
        }

        fn has_kind(&self, kind: &str) -> bool {
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
    }

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: forge_events::Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn register_returns_id_and_stores_combo() {
        let client = HotkeyClient::new_for_test(Some(true));
        let combo = HotkeyCombo::parse("Ctrl+F1").unwrap();
        let id = client.register(combo.clone()).await.unwrap();
        let combos = client.registered_combos();
        assert!(combos.iter().any(|(i, c)| i == &id && c == &combo));
    }

    #[tokio::test]
    async fn register_same_combo_twice_returns_already_registered() {
        let client = HotkeyClient::new_for_test(Some(true));
        let combo = HotkeyCombo::parse("Ctrl+A").unwrap();
        client.register(combo.clone()).await.unwrap();
        let err = client.register(combo.clone()).await.unwrap_err();
        assert!(matches!(err, HotkeyError::AlreadyRegistered { .. }));
    }

    #[tokio::test]
    async fn register_emits_hotkey_registered_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::new(backend),
            Some(true),
        );
        let combo = HotkeyCombo::parse("Ctrl+B").unwrap();
        client.register(combo).await.unwrap();
        assert!(publisher.has_kind("hotkey.registered"));
    }

    #[tokio::test]
    async fn unregister_removes_combo_and_emits_event() {
        let publisher = RecordingPublisher::new();
        let (backend, _tx) = MockPortalBackend::new();
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::new(backend),
            Some(true),
        );
        let combo = HotkeyCombo::parse("Ctrl+C").unwrap();
        let id = client.register(combo).await.unwrap();
        client.unregister(id).await.unwrap();
        assert!(client.registered_combos().is_empty());
        assert!(publisher.has_kind("hotkey.unregistered"));
    }

    #[tokio::test]
    async fn backend_conflict_returns_already_registered() {
        let (backend, _tx) = MockPortalBackend::new();
        backend.add_conflict("Alt+X");
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::new(struct_noop_publisher()),
            Arc::new(backend),
            Some(true),
        );
        let combo = HotkeyCombo::parse("Alt+X").unwrap();
        let err = client.register(combo).await.unwrap_err();
        assert!(matches!(err, HotkeyError::AlreadyRegistered { .. }));
    }

    #[tokio::test]
    async fn portal_available_reflects_passed_flag() {
        let client = HotkeyClient::new_for_test(Some(true));
        assert_eq!(client.portal_available(), Some(true));

        let (backend, _tx) = MockPortalBackend::new();
        let c2 = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::new(struct_noop_publisher()),
            Arc::new(backend),
            Some(false),
        );
        assert_eq!(c2.portal_available(), Some(false));
    }

    #[tokio::test]
    async fn fail_all_backend_register_returns_permission_denied() {
        let backend = MockFailAllBackend::new();
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::new(struct_noop_publisher()),
            Arc::new(backend),
            None,
        );
        let combo = HotkeyCombo::parse("Ctrl+D").unwrap();
        let err = client.register(combo).await.unwrap_err();
        assert!(matches!(err, HotkeyError::PermissionDenied));
    }

    #[tokio::test]
    async fn triggered_event_emitted_on_fired_rx() {
        let publisher = RecordingPublisher::new();
        let (backend, inject_tx) = MockPortalBackend::new();
        let client = HotkeyClient::start(
            HotkeyConfig::default(),
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            Arc::new(backend),
            Some(true),
        );
        let combo = HotkeyCombo::parse("Ctrl+F1").unwrap();
        let id = client.register(combo.clone()).await.unwrap();

        inject_tx
            .send(crate::backend::HotkeyFiredEvent {
                id,
                combo,
                timestamp_us: 0,
            })
            .await
            .unwrap();

        for _ in 0..8 {
            tokio::task::yield_now().await;
        }

        assert!(publisher.has_kind("hotkey.global.pressed"));
        let ev = publisher.find_kind("hotkey.global.pressed").unwrap();
        assert_eq!(ev.payload["combo"], "Ctrl+F1");
    }

    fn struct_noop_publisher() -> impl EventPublisher {
        struct Noop;
        impl EventPublisher for Noop {
            fn publish(&self, _: forge_events::Event) {}
        }
        Noop
    }
}
