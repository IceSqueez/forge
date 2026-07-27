use std::sync::Arc;

use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, EventBus, LiveViewerAggregatorHandle, QueueSchedulerHandle, ScriptRegistry,
    TriggerEvaluatorHandle,
};
use forge_storage::{DataProvider, Language};

use crate::integrations::{BuiltinRegistry, KickInstallSeed, ObsInstallSeed, YoutubeInstallSeed};

#[allow(dead_code)]
pub struct RuntimeHandles {
    pub rt_handle: tokio::runtime::Handle,
    pub backend: Arc<dyn DataProvider>,
    /// Resolved + persisted on the tokio side; `install_language` must run on the render thread.
    pub startup_language: Language,
    pub bus: Arc<EventBus>,
    pub script_registry: Arc<ScriptRegistry>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub builtins: BuiltinRegistry,
    /// `None` when Kick client credentials are absent.
    pub kick_install_seed: Option<KickInstallSeed>,
    /// `None` when YouTube client credentials are absent.
    pub youtube_install_seed: Option<YoutubeInstallSeed>,
    pub obs_install_seed: ObsInstallSeed,
    /// `None` when the server is disabled in settings or the boot bind failed.
    pub server: Option<forge_server::ServerHandle>,
    /// `None` only if speak-queue construction failed.
    pub speak: Option<forge_speak_queue::SpeakQueueHandle>,
    /// `None` only when the speak subsystem doesn't build.
    pub pipeline_config: Option<forge_speak_queue::PipelineConfigHandle>,
    /// Taken once by the boot bridge task; not `Clone`.
    pub speak_events: Option<forge_speak_queue::SpeakEventStream>,
    /// `None` in lockstep with `speak` (absent when the speak subsystem doesn't build).
    pub tts_registry: Option<Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub soundboard_player: Arc<forge_soundboard::SoundboardPlayer>,
}
