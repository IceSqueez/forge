use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::LiveViewerSource;
use forge_runtime::{IntegrationGate, LiveViewerAggregatorHandle};
use forge_storage::{
    SettingsRepo, StorageError, resolve_integration_enabled, set_integration_enabled,
};
use forge_types::{IntegrationId, PlatformId};
use futures_util::future::{BoxFuture, join_all};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{AbortHandle, JoinHandle};

use crate::integrations::{BuiltinObject, BuiltinRegistry};

const TEARDOWN_MARGIN: Duration = Duration::from_secs(5);
const TEARDOWN_BOUND: Duration =
    forge_platform_twitch::chat::SHUTDOWN_GRACE.saturating_add(TEARDOWN_MARGIN);
const START_BOUND: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    Disabled,
    Starting,
    Running,
    Stopping,
    Failed(String),
}

impl LifecycleState {
    pub fn is_settled(&self) -> bool {
        !matches!(self, Self::Starting | Self::Stopping)
    }
}

pub type LifecycleStates = BTreeMap<IntegrationId, LifecycleState>;

#[derive(Default)]
pub struct TaskGroup(Vec<AbortHandle>);

impl TaskGroup {
    pub fn track(&mut self, task: JoinHandle<()>) {
        self.0.push(task.abort_handle());
    }

    pub fn absorb(&mut self, other: TaskGroup) {
        self.0.extend(other.0);
    }

    pub fn abort_all(&self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

pub struct RunningIntegration {
    object: Option<BuiltinObject>,
    viewers: Option<(PlatformId, Box<dyn LiveViewerSource>)>,
    teardown: Option<BoxFuture<'static, ()>>,
}

impl RunningIntegration {
    pub fn idle() -> Self {
        Self {
            object: None,
            viewers: None,
            teardown: None,
        }
    }

    pub fn with_object(mut self, object: BuiltinObject) -> Self {
        self.object = Some(object);
        self
    }

    pub fn with_viewers(mut self, platform: PlatformId, source: Box<dyn LiveViewerSource>) -> Self {
        self.viewers = Some((platform, source));
        self
    }

    pub fn with_teardown(mut self, teardown: BoxFuture<'static, ()>) -> Self {
        self.teardown = Some(teardown);
        self
    }
}

#[async_trait]
pub trait IntegrationFactory: Send + Sync {
    fn id(&self) -> IntegrationId;
    async fn is_configured(&self) -> Result<bool, StorageError>;
    async fn start(&self) -> Result<RunningIntegration, String>;
}

struct Desire {
    enabled: Option<bool>,
    rebuild: bool,
    settled: Option<oneshot::Sender<LifecycleState>>,
}

struct Retained {
    viewer_platform: Option<PlatformId>,
    teardown: Option<BoxFuture<'static, ()>>,
}

#[derive(Clone)]
struct SlotHost {
    gate: IntegrationGate,
    builtins: BuiltinRegistry,
    live_viewers: LiveViewerAggregatorHandle,
    states: Arc<watch::Sender<LifecycleStates>>,
}

impl SlotHost {
    fn publish(&self, id: &IntegrationId, state: LifecycleState) {
        self.states.send_modify(|states| {
            states.insert(id.clone(), state);
        });
    }

    fn install(&self, id: &IntegrationId, running: RunningIntegration) -> Retained {
        let viewer_platform = running.viewers.map(|(platform, source)| {
            self.live_viewers.register(platform, source);
            platform
        });
        if let Some(object) = running.object {
            self.builtins.install(object);
        }
        self.gate.enable(id);
        Retained {
            viewer_platform,
            teardown: running.teardown,
        }
    }

    async fn retire(&self, id: &IntegrationId, retained: Retained) {
        self.builtins.remove(id);
        if let Some(platform) = retained.viewer_platform {
            self.live_viewers.unregister(platform);
        }
        if let Some(teardown) = retained.teardown {
            let mut teardown = tokio::spawn(teardown);
            if tokio::time::timeout(TEARDOWN_BOUND, &mut teardown)
                .await
                .is_err()
            {
                teardown.abort();
                tracing::warn!(integration = %id, "integration shutdown overran its bound; aborted it");
            }
        }
    }
}

async fn run_slot(
    id: IntegrationId,
    factory: Arc<dyn IntegrationFactory>,
    host: SlotHost,
    mut desires: mpsc::UnboundedReceiver<Desire>,
) {
    let mut desired = false;
    let mut retained: Option<Retained> = None;
    let mut current = LifecycleState::Disabled;
    while let Some(first) = desires.recv().await {
        let mut rebuild = false;
        let mut waiters = Vec::new();
        let mut absorb = |desire: Desire| {
            if let Some(enabled) = desire.enabled {
                desired = enabled;
            }
            rebuild |= desire.rebuild;
            waiters.extend(desire.settled);
        };
        absorb(first);
        while let Ok(next) = desires.try_recv() {
            absorb(next);
        }

        if !desired {
            host.gate.disable(id.clone());
            if let Some(live) = retained.take() {
                host.publish(&id, LifecycleState::Stopping);
                host.retire(&id, live).await;
            }
            current = LifecycleState::Disabled;
        } else if retained.is_none() || rebuild {
            if let Some(live) = retained.take() {
                host.publish(&id, LifecycleState::Stopping);
                host.retire(&id, live).await;
            }
            host.publish(&id, LifecycleState::Starting);
            let started = tokio::time::timeout(START_BOUND, factory.start())
                .await
                .unwrap_or_else(|_| {
                    Err(format!(
                        "did not start within {} seconds",
                        START_BOUND.as_secs()
                    ))
                });
            current = match started {
                Ok(running) => {
                    retained = Some(host.install(&id, running));
                    LifecycleState::Running
                }
                Err(reason) => {
                    tracing::warn!(integration = %id, error = %reason, "integration failed to start");
                    host.gate.disable(id.clone());
                    LifecycleState::Failed(reason)
                }
            };
        }
        host.publish(&id, current.clone());
        for waiter in waiters {
            let _ = waiter.send(current.clone());
        }
    }
    if let Some(live) = retained.take() {
        host.retire(&id, live).await;
    }
}

struct SupervisorInner {
    rt_handle: tokio::runtime::Handle,
    settings: Arc<dyn SettingsRepo>,
    builtins: BuiltinRegistry,
    factories: Vec<Arc<dyn IntegrationFactory>>,
    slots: HashMap<IntegrationId, mpsc::UnboundedSender<Desire>>,
    states: watch::Receiver<LifecycleStates>,
}

#[derive(Clone)]
pub struct IntegrationSupervisor {
    inner: Arc<SupervisorInner>,
}

impl IntegrationSupervisor {
    pub fn launch(
        factories: Vec<Arc<dyn IntegrationFactory>>,
        settings: Arc<dyn SettingsRepo>,
        gate: IntegrationGate,
        builtins: BuiltinRegistry,
        live_viewers: LiveViewerAggregatorHandle,
    ) -> Self {
        let initial: LifecycleStates = factories
            .iter()
            .map(|factory| (factory.id(), LifecycleState::Disabled))
            .collect();
        let (states_tx, states) = watch::channel(initial);
        let host = SlotHost {
            gate,
            builtins: builtins.clone(),
            live_viewers,
            states: Arc::new(states_tx),
        };
        let mut slots = HashMap::new();
        for factory in &factories {
            let id = factory.id();
            host.gate.disable(id.clone());
            let (desires_tx, desires_rx) = mpsc::unbounded_channel();
            tokio::spawn(run_slot(
                id.clone(),
                Arc::clone(factory),
                host.clone(),
                desires_rx,
            ));
            slots.insert(id, desires_tx);
        }
        Self {
            inner: Arc::new(SupervisorInner {
                rt_handle: tokio::runtime::Handle::current(),
                settings,
                builtins,
                factories,
                slots,
                states,
            }),
        }
    }

    pub async fn boot(&self) {
        let pending = self.inner.factories.iter().map(|factory| {
            let factory = Arc::clone(factory);
            let supervisor = self.clone();
            async move {
                let id = factory.id();
                let enabled = match resolve_integration_enabled(
                    supervisor.inner.settings.as_ref(),
                    &id,
                    factory.is_configured(),
                )
                .await
                {
                    Ok(enabled) => enabled,
                    Err(e) => {
                        tracing::warn!(integration = %id, error = %e, "could not resolve whether the integration is enabled");
                        factory.is_configured().await.unwrap_or(false)
                    }
                };
                supervisor.desire(&id, Some(enabled), false).await;
            }
        });
        join_all(pending).await;
    }

    pub fn slot(&self, id: &IntegrationId) -> Option<IntegrationSlot> {
        self.inner.slots.contains_key(id).then(|| IntegrationSlot {
            supervisor: self.clone(),
            id: id.clone(),
        })
    }

    pub fn state_of(&self, id: &IntegrationId) -> LifecycleState {
        self.inner
            .states
            .borrow()
            .get(id)
            .cloned()
            .unwrap_or(LifecycleState::Disabled)
    }

    pub fn watch(&self) -> LifecycleWatch {
        LifecycleWatch(self.inner.states.clone())
    }

    async fn desire(
        &self,
        id: &IntegrationId,
        enabled: Option<bool>,
        rebuild: bool,
    ) -> LifecycleState {
        let Some(slot) = self.inner.slots.get(id) else {
            return LifecycleState::Disabled;
        };
        let (settled_tx, settled_rx) = oneshot::channel();
        let desire = Desire {
            enabled,
            rebuild,
            settled: Some(settled_tx),
        };
        if slot.send(desire).is_err() {
            return self.state_of(id);
        }
        settled_rx.await.unwrap_or_else(|_| self.state_of(id))
    }
}

pub struct LifecycleWatch(watch::Receiver<LifecycleStates>);

impl LifecycleWatch {
    pub fn current(&mut self) -> LifecycleStates {
        self.0.borrow_and_update().clone()
    }

    pub async fn changed(&mut self) -> Option<LifecycleStates> {
        self.0.changed().await.ok()?;
        Some(self.0.borrow_and_update().clone())
    }
}

#[derive(Clone)]
pub struct IntegrationSlot {
    supervisor: IntegrationSupervisor,
    id: IntegrationId,
}

impl IntegrationSlot {
    pub fn state(&self) -> LifecycleState {
        self.supervisor.state_of(&self.id)
    }

    pub fn id(&self) -> &IntegrationId {
        &self.id
    }

    pub fn watch(&self) -> LifecycleWatch {
        self.supervisor.watch()
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<LifecycleState, String> {
        set_integration_enabled(self.supervisor.inner.settings.as_ref(), &self.id, enabled)
            .await
            .map_err(|e| e.to_string())?;
        Ok(self.supervisor.desire(&self.id, Some(enabled), false).await)
    }

    pub async fn activate(&self) -> Result<BuiltinObject, String> {
        set_integration_enabled(self.supervisor.inner.settings.as_ref(), &self.id, true)
            .await
            .map_err(|e| e.to_string())?;
        match self.supervisor.desire(&self.id, Some(true), true).await {
            LifecycleState::Running => self
                .supervisor
                .inner
                .builtins
                .get(&self.id)
                .ok_or_else(|| format!("{} started without a signed-in session", self.id)),
            LifecycleState::Failed(reason) => Err(reason),
            other => Err(format!("{} settled as {other:?}", self.id)),
        }
    }

    pub async fn rebuild(&self) -> LifecycleState {
        self.supervisor.desire(&self.id, None, true).await
    }

    pub fn request_enable(&self) {
        if matches!(
            self.state(),
            LifecycleState::Running | LifecycleState::Starting
        ) {
            return;
        }
        let slot = self.clone();
        self.supervisor.inner.rt_handle.spawn(async move {
            if let Err(e) = slot.set_enabled(true).await {
                tracing::warn!(integration = %slot.id, error = %e, "could not enable the integration");
            }
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{Event, EventPublisher};
    use forge_platform_core::SectionIcon;
    use forge_runtime::spawn_live_viewer_aggregator;
    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::integration_enabled_key;

    use super::*;

    #[derive(Default)]
    struct MemorySettings(Mutex<HashMap<String, String>>);

    impl MemorySettings {
        fn value(&self, key: &str) -> Option<String> {
            self.0.lock().unwrap().get(key).cloned()
        }
    }

    #[async_trait]
    impl SettingsRepo for MemorySettings {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.value(key))
        }
        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(key).is_some())
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct NoopPublisher;

    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    #[derive(Default)]
    struct Behaviour {
        configured: bool,
        hold_start: Option<oneshot::Receiver<()>>,
        fail_once: Option<String>,
        teardown_hangs: bool,
        installs_object: bool,
    }

    struct FakeFactory {
        id: IntegrationId,
        behaviour: Mutex<Behaviour>,
        configured: bool,
        teardown_hangs: bool,
        installs_object: bool,
        starts: AtomicUsize,
        teardowns: Arc<AtomicUsize>,
        held_by_tasks: Arc<()>,
    }

    impl FakeFactory {
        fn new(id: &'static str, behaviour: Behaviour) -> Arc<Self> {
            Arc::new(Self {
                id: IntegrationId::from_static(id),
                configured: behaviour.configured,
                teardown_hangs: behaviour.teardown_hangs,
                installs_object: behaviour.installs_object,
                behaviour: Mutex::new(behaviour),
                starts: AtomicUsize::new(0),
                teardowns: Arc::new(AtomicUsize::new(0)),
                held_by_tasks: Arc::new(()),
            })
        }

        fn starts(&self) -> usize {
            self.starts.load(Ordering::SeqCst)
        }

        fn teardowns(&self) -> usize {
            self.teardowns.load(Ordering::SeqCst)
        }

        fn live_tasks(&self) -> usize {
            Arc::strong_count(&self.held_by_tasks) - 1
        }
    }

    #[async_trait]
    impl IntegrationFactory for FakeFactory {
        fn id(&self) -> IntegrationId {
            self.id.clone()
        }

        async fn is_configured(&self) -> Result<bool, StorageError> {
            Ok(self.configured)
        }

        async fn start(&self) -> Result<RunningIntegration, String> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            let (hold, failure) = {
                let mut behaviour = self.behaviour.lock().unwrap();
                (behaviour.hold_start.take(), behaviour.fail_once.take())
            };
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            if let Some(reason) = failure {
                return Err(reason);
            }
            let mut tasks = TaskGroup::default();
            let held = Arc::clone(&self.held_by_tasks);
            tasks.track(tokio::spawn(async move {
                let _held = held;
                std::future::pending::<()>().await;
            }));
            let teardowns = Arc::clone(&self.teardowns);
            let hangs = self.teardown_hangs;
            let mut running = RunningIntegration::idle().with_teardown(Box::pin(async move {
                tasks.abort_all();
                teardowns.fetch_add(1, Ordering::SeqCst);
                if hangs {
                    std::future::pending::<()>().await;
                }
            }));
            if self.installs_object {
                let client = forge_discord::DiscordClient::new(
                    forge_discord::DiscordConfig::default(),
                    Arc::new(NoopPublisher),
                    Arc::new(MockCredentialsRepo::new()),
                );
                running = running.with_object(BuiltinObject {
                    icon: SectionIcon::new("brand-discord"),
                    status: client.clone(),
                    health: client.clone(),
                    content: client.clone(),
                    quick: client,
                    control: None,
                    collections: None,
                    obs_client: None,
                    vtube_client: None,
                });
            }
            Ok(running)
        }
    }

    struct Harness {
        supervisor: IntegrationSupervisor,
        settings: Arc<MemorySettings>,
        gate: IntegrationGate,
        builtins: BuiltinRegistry,
    }

    impl Harness {
        fn launch(factory: &Arc<FakeFactory>) -> Self {
            let settings = Arc::new(MemorySettings::default());
            let gate = IntegrationGate::new();
            let builtins = BuiltinRegistry::default();
            let supervisor = IntegrationSupervisor::launch(
                vec![Arc::clone(factory) as Arc<dyn IntegrationFactory>],
                Arc::clone(&settings) as Arc<dyn SettingsRepo>,
                gate.clone(),
                builtins.clone(),
                spawn_live_viewer_aggregator(),
            );
            Self {
                supervisor,
                settings,
                gate,
                builtins,
            }
        }

        fn slot(&self, factory: &FakeFactory) -> IntegrationSlot {
            self.supervisor.slot(&factory.id).expect("declared slot")
        }
    }

    async fn settle_tasks(done: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if done() {
                return true;
            }
            tokio::task::yield_now().await;
        }
        done()
    }

    #[tokio::test]
    async fn a_launched_integration_stays_stopped_with_its_steps_gated_until_enabled() {
        let factory = FakeFactory::new("twitch", Behaviour::default());

        let harness = Harness::launch(&factory);

        assert_eq!(
            harness.supervisor.state_of(&factory.id),
            LifecycleState::Disabled
        );
        assert!(harness.gate.is_disabled(&factory.id));
        assert_eq!(factory.starts(), 0);
    }

    #[tokio::test]
    async fn boot_starts_only_integrations_resolved_as_enabled() {
        for (stored, configured, expect_running) in [
            (None, true, true),
            (None, false, false),
            (Some("false"), true, false),
            (Some("true"), false, true),
        ] {
            let factory = FakeFactory::new(
                "twitch",
                Behaviour {
                    configured,
                    ..Behaviour::default()
                },
            );
            let harness = Harness::launch(&factory);
            if let Some(value) = stored {
                harness
                    .settings
                    .set_string(&integration_enabled_key(&factory.id), value)
                    .await
                    .unwrap();
            }

            harness.supervisor.boot().await;

            let case = format!("stored {stored:?}, configured {configured}");
            assert_eq!(
                harness.supervisor.state_of(&factory.id) == LifecycleState::Running,
                expect_running,
                "{case}"
            );
            assert_eq!(factory.starts(), usize::from(expect_running), "{case}");
            assert_eq!(
                harness
                    .settings
                    .value(&integration_enabled_key(&factory.id))
                    .as_deref(),
                Some(if expect_running { "true" } else { "false" }),
                "{case}"
            );
        }
    }

    #[tokio::test]
    async fn enabling_starts_installs_and_opens_the_gate_and_disabling_reverses_all_of_it() {
        let factory = FakeFactory::new(
            "discord",
            Behaviour {
                installs_object: true,
                ..Behaviour::default()
            },
        );
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);

        let enabled = slot.set_enabled(true).await.unwrap();

        assert_eq!(enabled, LifecycleState::Running);
        assert!(!harness.gate.is_disabled(&factory.id));
        assert!(harness.builtins.get(&factory.id).is_some());
        assert_eq!(
            harness
                .settings
                .value(&integration_enabled_key(&factory.id))
                .as_deref(),
            Some("true")
        );

        let disabled = slot.set_enabled(false).await.unwrap();

        assert_eq!(disabled, LifecycleState::Disabled);
        assert!(harness.gate.is_disabled(&factory.id));
        assert!(harness.builtins.get(&factory.id).is_none());
        assert_eq!(factory.teardowns(), 1);
        assert_eq!(
            harness
                .settings
                .value(&integration_enabled_key(&factory.id))
                .as_deref(),
            Some("false")
        );
    }

    #[tokio::test]
    async fn enabling_an_already_running_integration_does_not_restart_it() {
        let factory = FakeFactory::new("twitch", Behaviour::default());
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);

        slot.set_enabled(true).await.unwrap();
        let again = slot.set_enabled(true).await.unwrap();

        assert_eq!(again, LifecycleState::Running);
        assert_eq!(factory.starts(), 1);
        assert_eq!(factory.teardowns(), 0);
    }

    #[tokio::test]
    async fn disabling_a_stopped_integration_runs_no_teardown() {
        let factory = FakeFactory::new("twitch", Behaviour::default());
        let harness = Harness::launch(&factory);

        let state = harness.slot(&factory).set_enabled(false).await.unwrap();

        assert_eq!(state, LifecycleState::Disabled);
        assert_eq!(factory.teardowns(), 0);
    }

    async fn toggle_while_starting(final_enabled: bool) -> (Arc<FakeFactory>, Harness) {
        let (release, hold) = oneshot::channel();
        let factory = FakeFactory::new(
            "twitch",
            Behaviour {
                hold_start: Some(hold),
                ..Behaviour::default()
            },
        );
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);
        let first = tokio::spawn({
            let slot = slot.clone();
            async move { slot.set_enabled(true).await }
        });
        assert!(
            settle_tasks(|| factory.starts() == 1).await,
            "start never began"
        );

        let mut toggles = Vec::new();
        for enabled in [false, true, false, true, final_enabled] {
            let slot = slot.clone();
            toggles.push(tokio::spawn(async move { slot.set_enabled(enabled).await }));
            for _ in 0..8 {
                tokio::task::yield_now().await;
            }
        }
        assert_eq!(
            harness.supervisor.state_of(&factory.id),
            LifecycleState::Starting
        );
        release.send(()).unwrap();

        first.await.unwrap().unwrap();
        for toggle in toggles {
            toggle.await.unwrap().unwrap();
        }
        (factory, harness)
    }

    #[tokio::test]
    async fn rapid_toggles_during_a_start_coalesce_to_the_last_wish_without_restarting() {
        let (factory, harness) = toggle_while_starting(true).await;

        assert_eq!(
            harness.supervisor.state_of(&factory.id),
            LifecycleState::Running
        );
        assert_eq!(factory.starts(), 1);
        assert_eq!(factory.teardowns(), 0);
        assert!(!harness.gate.is_disabled(&factory.id));
    }

    #[tokio::test]
    async fn disabling_during_a_start_tears_the_fresh_instance_down_once_it_is_up() {
        let (factory, harness) = toggle_while_starting(false).await;

        assert_eq!(
            harness.supervisor.state_of(&factory.id),
            LifecycleState::Disabled
        );
        assert_eq!(factory.starts(), 1);
        assert_eq!(factory.teardowns(), 1);
        assert!(harness.gate.is_disabled(&factory.id));
        assert!(settle_tasks(|| factory.live_tasks() == 0).await);
    }

    #[tokio::test(start_paused = true)]
    async fn a_teardown_that_never_finishes_is_abandoned_at_the_bound() {
        let factory = FakeFactory::new(
            "twitch",
            Behaviour {
                teardown_hangs: true,
                ..Behaviour::default()
            },
        );
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);
        slot.set_enabled(true).await.unwrap();
        let began = tokio::time::Instant::now();

        let state = tokio::time::timeout(TEARDOWN_BOUND * 2, slot.set_enabled(false))
            .await
            .expect("disable must settle once the teardown bound passes")
            .unwrap();

        assert_eq!(state, LifecycleState::Disabled);
        assert_eq!(began.elapsed(), TEARDOWN_BOUND);
        assert!(harness.gate.is_disabled(&factory.id));
    }

    #[tokio::test]
    async fn repeated_enable_disable_cycles_leave_no_integration_task_alive() {
        let factory = FakeFactory::new("twitch", Behaviour::default());
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);

        for cycle in 0..3 {
            slot.set_enabled(true).await.unwrap();
            assert!(
                settle_tasks(|| factory.live_tasks() == 1).await,
                "cycle {cycle}: the running instance owns exactly one task"
            );
            slot.set_enabled(false).await.unwrap();
            assert!(
                settle_tasks(|| factory.live_tasks() == 0).await,
                "cycle {cycle}: {} task(s) survived the disable",
                factory.live_tasks()
            );
        }
        assert_eq!((factory.starts(), factory.teardowns()), (3, 3));
    }

    #[tokio::test]
    async fn a_failed_start_keeps_the_gate_closed_and_a_later_enable_retries() {
        let factory = FakeFactory::new(
            "twitch",
            Behaviour {
                fail_once: Some("token rejected".to_owned()),
                ..Behaviour::default()
            },
        );
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);

        let failed = slot.set_enabled(true).await.unwrap();

        assert_eq!(failed, LifecycleState::Failed("token rejected".to_owned()));
        assert!(harness.gate.is_disabled(&factory.id));

        let retried = slot.set_enabled(true).await.unwrap();

        assert_eq!(retried, LifecycleState::Running);
        assert_eq!(factory.starts(), 2);
        assert!(!harness.gate.is_disabled(&factory.id));
    }

    #[tokio::test]
    async fn activating_a_running_integration_rebuilds_it_and_hands_back_the_new_object() {
        let factory = FakeFactory::new(
            "discord",
            Behaviour {
                installs_object: true,
                ..Behaviour::default()
            },
        );
        let harness = Harness::launch(&factory);
        let slot = harness.slot(&factory);
        slot.set_enabled(true).await.unwrap();

        let object = slot.activate().await;

        assert!(object.is_ok());
        assert_eq!((factory.starts(), factory.teardowns()), (2, 1));
        assert!(settle_tasks(|| factory.live_tasks() == 1).await);
    }

    #[tokio::test]
    async fn activating_an_integration_that_starts_without_a_session_reports_an_error() {
        let factory = FakeFactory::new("twitch", Behaviour::default());
        let harness = Harness::launch(&factory);

        let result = harness.slot(&factory).activate().await;

        assert!(result.is_err());
        assert_eq!(
            harness.supervisor.state_of(&factory.id),
            LifecycleState::Running
        );
    }
}
