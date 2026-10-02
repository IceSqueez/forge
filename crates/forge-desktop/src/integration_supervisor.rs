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
