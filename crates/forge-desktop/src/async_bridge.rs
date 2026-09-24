use std::cell::Cell;
use std::fmt::Display;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use forge_components::ToastKind;
use forge_events::{Event, EventsError};
use forge_runtime::{ActionEngineHandle, EventBus, EventSubscription};
use forge_types::{SubActionOutcome, SubActionStep};
use gpui::{App, AsyncApp, Context, Entity, SharedString};
use tokio::runtime::Handle;

use crate::toasts::PushToast;

const BRIDGE_DRAIN_CAP: usize = 128;

pub const SLIDER_PERSIST_DEBOUNCE: Duration = Duration::from_millis(400);

pub enum EventBatch {
    Ready(Vec<Event>),
    Closed,
}

pub async fn recv_event_batch(sub: &mut EventSubscription) -> EventBatch {
    let first = loop {
        match sub.recv().await {
            Ok(event) => break event,
            Err(EventsError::LaggingReceiver) => {
                tracing::warn!("event bridge lagged; dropped events");
                continue;
            }
            Err(_) => return EventBatch::Closed,
        }
    };
    let mut batch = Vec::with_capacity(1);
    batch.push(first);
    while batch.len() < BRIDGE_DRAIN_CAP {
        match sub.try_recv() {
            Ok(Some(event)) => batch.push(event),
            _ => break,
        }
    }
    EventBatch::Ready(batch)
}

pub async fn run_quick_step(
    engine: ActionEngineHandle,
    step: SubActionStep,
    builtin_id: String,
    label: String,
) -> Result<(), String> {
    let pending = engine
        .execute_quick_action(step, builtin_id, label, None)
        .await
        .map_err(|err| err.to_string())?;
    match pending.outcome().await.map_err(|err| err.to_string())? {
        SubActionOutcome::Success => Ok(()),
        SubActionOutcome::Failed(reason) | SubActionOutcome::Skipped(reason) => Err(reason),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BridgeFlow {
    Continue,
    Stop,
}

/// Drains `bus` batches into `apply` until the bus closes or `apply` returns `Stop`; a lagging receiver warns and keeps receiving (broadcast semantics).
pub async fn drain_events<F>(bus: &EventBus, cx: &mut AsyncApp, apply: F)
where
    F: FnMut(&[Event], &mut AsyncApp) -> BridgeFlow,
{
    drain_subscription(bus.subscribe(), cx, apply).await;
}

pub async fn drain_subscription<F>(mut sub: EventSubscription, cx: &mut AsyncApp, mut apply: F)
where
    F: FnMut(&[Event], &mut AsyncApp) -> BridgeFlow,
{
    while let EventBatch::Ready(batch) = recv_event_batch(&mut sub).await {
        if apply(&batch, cx) == BridgeFlow::Stop {
            break;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorSink {
    Toast,
    Banner,
    Silent,
}

impl ErrorSink {
    pub fn report(self, message: String, cx: &mut App) -> Option<String> {
        match self {
            ErrorSink::Toast => {
                cx.push_toast(ToastKind::Error, message);
                None
            }
            ErrorSink::Banner => Some(message),
            ErrorSink::Silent => None,
        }
    }
}

pub fn report_failure<V, F, E>(
    handle: &Handle,
    fut: F,
    sink: ErrorSink,
    context: impl Into<SharedString>,
    cx: &mut Context<V>,
) where
    V: 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    let context = context.into();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        if let Err(e) = fut.await {
            tracing::warn!(error = %e, context = %context, "async operation failed");
            let _ = tx.send(context);
        }
    });
    cx.spawn(async move |_this, cx| {
        if let Ok(message) = rx.await {
            cx.update(|cx| {
                sink.report(message.to_string(), cx);
            });
        }
    })
    .detach();
}

pub fn detached<F, E>(handle: &Handle, context: &'static str, fut: F)
where
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    handle.spawn(async move {
        if let Err(e) = fut.await {
            tracing::warn!(error = %e, context = %context, "detached write failed");
        }
    });
}

pub async fn open_path(target: impl AsRef<std::ffi::OsStr> + Send + 'static) -> Result<(), String> {
    tokio::task::spawn_blocking(move || open::that(target))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

pub fn open_external<V>(
    handle: &Handle,
    target: impl Into<String>,
    sink: ErrorSink,
    context: impl Into<SharedString>,
    cx: &mut Context<V>,
) where
    V: 'static,
{
    let target = target.into();
    report_failure(handle, open_path(target), sink, context, cx);
}

pub const DIALOG_CANCELLED: &str = "dialog cancelled";

pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<&'static str>,
}

pub async fn pick_file(filter: Option<DialogFilter>) -> Result<PathBuf, String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(filter) = filter {
        dialog = dialog.add_filter(filter.name, &filter.extensions);
    }
    dialog
        .pick_file()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub async fn pick_folder() -> Result<PathBuf, String> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub async fn save_file(
    filter: Option<DialogFilter>,
    default_name: Option<String>,
) -> Result<PathBuf, String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(filter) = filter {
        dialog = dialog.add_filter(filter.name, &filter.extensions);
    }
    if let Some(name) = default_name {
        dialog = dialog.set_file_name(name);
    }
    dialog
        .save_file()
        .await
        .map(|handle| handle.path().to_path_buf())
        .ok_or_else(|| DIALOG_CANCELLED.to_owned())
}

pub fn spawn_dialog<V, Fut, T>(
    handle: &Handle,
    dialog: Fut,
    on_result: impl FnOnce(&mut V, Result<T, String>, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = Result<T, String>> + Send + 'static,
{
    run_async(handle, dialog, on_result, cx);
}

/// Runs `fut` on `handle`, then applies its output to the calling view; a dropped view or a dropped task silently no-ops (the view is held weakly).
pub fn run_async<V, Fut, T>(
    handle: &Handle,
    fut: Fut,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await);
    });
    cx.spawn(async move |this, cx| {
        if let Ok(result) = rx.await {
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

/// Like `run_async` but from an `&mut App` with an explicit `view`; the strong `Entity` keeps the view alive until the future resolves.
pub fn run_async_entity<V, Fut, T>(
    handle: &Handle,
    view: Entity<V>,
    fut: Fut,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    app: &mut App,
) where
    V: 'static,
    T: Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await);
    });
    app.spawn(async move |cx| {
        if let Ok(result) = rx.await {
            view.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

/// Runs blocking `work` on `handle`'s blocking pool, then applies its output to the calling view (held weakly).
pub fn run_blocking<V, Work, T>(
    handle: &Handle,
    work: Work,
    apply: impl FnOnce(&mut V, T, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    T: Send + 'static,
    Work: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn_blocking(move || {
        let _ = tx.send(work());
    });
    cx.spawn(async move |this, cx| {
        if let Ok(result) = rx.await {
            let _ = this.update(cx, |this, cx| apply(this, result, cx));
        }
    })
    .detach();
}

pub struct Debounced {
    generation: Arc<AtomicU64>,
    delay: Duration,
}

impl Debounced {
    pub fn new(delay: Duration) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            delay,
        }
    }

    /// Runs `fut` after `delay` of quiet, superseding any still-pending write so only the last value of a burst reaches disk (last-write-wins). The write is dispatched on `handle`, so it outlives a drop of the calling view mid-debounce; failures log (no view is guaranteed to survive to toast on).
    pub fn schedule<F, E>(&self, handle: &Handle, context: impl Into<SharedString>, fut: F)
    where
        F: Future<Output = Result<(), E>> + Send + 'static,
        E: Display + Send + 'static,
    {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = Arc::clone(&self.generation);
        let delay = self.delay;
        let context = context.into();
        handle.spawn(async move {
            tokio::time::sleep(delay).await;
            if generation.load(Ordering::SeqCst) != ticket {
                return;
            }
            if let Err(e) = fut.await {
                tracing::warn!(error = %e, context = %context, "debounced write failed");
            }
        });
    }
}

#[derive(Default)]
pub struct Generation(Cell<u64>);

impl Generation {
    pub fn next(&self) -> u64 {
        let next = self.0.get().wrapping_add(1);
        self.0.set(next);
        next
    }

    /// True only for the most recently issued ticket; a stale async result (an older request that resolved after a newer one) compares false and must be discarded.
    pub fn is_current(&self, ticket: u64) -> bool {
        self.0.get() == ticket
    }
}

pub fn optimistic<V, S, F, E>(
    handle: &Handle,
    snapshot: S,
    fut: F,
    restore: impl FnOnce(&mut V, S, String, &mut Context<V>) + Send + 'static,
    cx: &mut Context<V>,
) where
    V: 'static,
    S: Send + 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
    E: Display + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle.spawn(async move {
        let _ = tx.send(fut.await.map_err(|e| e.to_string()));
    });
    cx.spawn(async move |this, cx| {
        if let Ok(Err(message)) = rx.await {
            tracing::warn!(error = %message, "optimistic persist failed");
            let _ = this.update(cx, |this, cx| {
                restore(this, snapshot, message, cx);
                cx.notify();
            });
        }
    })
    .detach();
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_registry::{
        FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
        SubActionRunner,
    };
    use forge_runtime::{ActionCancelRegistry, DispatchError, spawn_action_engine};
    use forge_storage::history::MockHistoryRepo;
    use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

    use super::*;
    use crate::test_support::{StubActions, StubEventLog, runtime};

    const SUCCEEDS: &str = "test.succeeds";
    const FAILS: &str = "test.fails";
    const SKIPS: &str = "test.skips";
    const FAILURE_REASON: &str = "scene not found";
    const SKIP_REASON: &str = "not connected";
    const ENGINE_EXIT_YIELDS: usize = 8;

    struct FixedOutcome {
        id: &'static str,
        outcome: SubActionOutcome,
    }

    #[async_trait::async_trait]
    impl SubActionRunner for FixedOutcome {
        fn id(&self) -> &str {
            self.id
        }
        fn category(&self) -> SubActionCategory {
            SubActionCategory::Util
        }
        fn label(&self) -> &str {
            self.id
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            ""
        }
        fn default_config(&self) -> SubActionConfig {
            SubActionConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _: &SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            (
                StepTimer::start(ctx, self.id).finish(self.outcome.clone()),
                None,
            )
        }
    }

    fn engine() -> ActionEngineHandle {
        let mut registry = SubActionRegistry::new();
        for (id, outcome) in [
            (SUCCEEDS, SubActionOutcome::Success),
            (FAILS, SubActionOutcome::Failed(FAILURE_REASON.to_owned())),
            (SKIPS, SubActionOutcome::Skipped(SKIP_REASON.to_owned())),
        ] {
            registry
                .register(Box::new(FixedOutcome { id, outcome }))
                .expect("each fixed-outcome runner has its own id");
        }
        let mut history = MockHistoryRepo::new();
        history.expect_save().returning(|_| Ok(()));
        spawn_action_engine(
            EventBus::new(Arc::new(StubEventLog)),
            Arc::new(StubActions),
            Arc::new(history),
            Arc::new(registry),
            Arc::new(ActionCancelRegistry::new()),
        )
    }

    fn quick_step(kind_id: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    async fn run(engine: ActionEngineHandle, kind_id: &str) -> Result<(), String> {
        run_quick_step(
            engine,
            quick_step(kind_id),
            "obs".to_owned(),
            kind_id.to_owned(),
        )
        .await
    }

    #[test]
    fn a_quick_step_settles_ok_only_when_the_step_succeeded() {
        let rt = runtime();
        for (kind_id, expected) in [
            (SUCCEEDS, Ok(())),
            (FAILS, Err(FAILURE_REASON.to_owned())),
            (SKIPS, Err(SKIP_REASON.to_owned())),
        ] {
            let settled = rt.block_on(async { run(engine(), kind_id).await });
            assert_eq!(settled, expected, "{kind_id}");
        }
    }

    #[test]
    fn a_quick_step_the_stopped_engine_never_ran_settles_as_an_error() {
        let rt = runtime();
        let not_accepted = rt.block_on(async {
            let engine = engine();
            engine.clone().shutdown();
            for _ in 0..ENGINE_EXIT_YIELDS {
                tokio::task::yield_now().await;
            }
            run(engine, SUCCEEDS).await
        });
        let dropped_from_intake = rt.block_on(async {
            let engine = engine();
            engine.clone().shutdown();
            run(engine, SUCCEEDS).await
        });

        assert_eq!(
            (not_accepted, dropped_from_intake),
            (
                Err(DispatchError::ChannelClosed.to_string()),
                Err(DispatchError::NoOutcome.to_string()),
            )
        );
    }
}
