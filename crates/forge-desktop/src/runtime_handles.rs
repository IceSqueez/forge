use std::collections::HashMap;
use std::sync::Arc;

use forge_platform_core::BuiltinId;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, EventBus, LiveViewerAggregatorHandle, QueueSchedulerHandle, ScriptRegistry,
    TriggerEvaluatorHandle,
};
use forge_storage::DataProvider;

use crate::integrations::BuiltinObject;

/// Inbound grouping of the runtime's individually-exposed command handles, assembled
/// once at boot and handed to the shell's `Ready` state. There is no aggregate handle
/// in the runtime; the shell owns this bundle so screens reach the runtime through it
/// (later phases) while the root stays within its field budget. Holding it alive keeps
/// the engine/scheduler/evaluator tasks' channels open for the app's lifetime. No raw
/// tokio channel crosses the boundary — these are the runtime's own public handles.
// The command handles are the runtime write-edge; each is read once its owning screen
// is wired, so the whole surface is held rather than consumed field-by-field yet.
#[allow(dead_code)]
pub struct RuntimeHandles {
    /// The tokio runtime handle owned by `main`. Screens that dispatch a runtime
    /// verb doing real network I/O (a lifecycle control's reconnect/disconnect/
    /// refresh, a quick action) spawn onto this handle so the future runs with a
    /// tokio reactor, rather than gpui's foreground executor which has none.
    pub rt_handle: tokio::runtime::Handle,
    pub backend: Arc<dyn DataProvider>,
    pub bus: Arc<EventBus>,
    pub script_registry: Arc<ScriptRegistry>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub builtins: HashMap<BuiltinId, BuiltinObject>,
    /// The hosted WS+HTTP server, bound at boot only when the persisted server
    /// settings enable it; `None` when disabled or when the bind failed (no I/O).
    /// The server console polls its `snapshot()` and dispatches lifecycle verbs
    /// through it; every other screen ignores it.
    pub server: Option<forge_server::ServerHandle>,
    /// The speak-queue command handle; `None` only if construction itself failed.
    /// Screens dispatch `SpeakCommand`s directly through this (Enqueue/Skip/Clear/...).
    pub speak: Option<forge_speak_queue::SpeakQueueHandle>,
    /// Hot-mutable via TTS Triggers screen saves; read by the `Speak` sub-action
    /// runner on every dispatch to gate reward/command/bits-sourced speech.
    pub tts_trigger_settings: forge_runtime::TtsTriggerSettingsHandle,
    /// The live message-preprocessing pipeline config, in lockstep with `speak`
    /// (`None` only when the speak subsystem doesn't build). The Filters screen swaps
    /// it on save to hot-reload the running queue.
    pub pipeline_config: Option<forge_speak_queue::PipelineConfigHandle>,
    /// The speak queue's initial event subscription. Not `Clone` — taken once by the
    /// boot-time bridge task that feeds `SpeakEvent`s into the dashboard entity.
    pub speak_events: Option<forge_speak_queue::SpeakEventStream>,
    /// Shared with the running speak-queue actor; the Cloud Engines screen registers
    /// credentialed engines into it live. In lockstep with `speak`.
    pub tts_registry: Option<Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>>,
}
