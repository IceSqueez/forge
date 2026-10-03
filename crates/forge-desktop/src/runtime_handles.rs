use std::sync::Arc;

use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, EventBus, LiveViewerAggregatorHandle, OverlayServiceHandle,
    QueueSchedulerHandle, ScriptRegistry, StreamLiveHandle, TriggerEvaluatorHandle,
};
use forge_storage::{CredentialsKeyLoss, DataProvider, Language};

use crate::audio_router::AudioRouter;
use crate::integration_supervisor::IntegrationSupervisor;
use crate::integrations::{BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
use crate::log_tail::LogTail;
use crate::voice_gate::VoiceGateOwner;

#[allow(dead_code)]
pub struct RuntimeHandles {
    pub rt_handle: tokio::runtime::Handle,
    pub log_tail: LogTail,
    pub backend: Arc<dyn DataProvider>,
    pub startup_language: Language,
    pub anonymous_donor: forge_types::Shared<String>,
    pub credentials_key_loss: Option<CredentialsKeyLoss>,
    pub bus: Arc<EventBus>,
    pub script_registry: Arc<ScriptRegistry>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub overlay_kinds: Arc<OverlayKindRegistry>,
    pub overlays: OverlayServiceHandle,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub stream_live: StreamLiveHandle,
    pub builtins: BuiltinRegistry,
    pub integrations: IntegrationSupervisor,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Arc<forge_discord::DiscordClient>,
    pub donation_services: crate::donation_services::DonationServices,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub server: Option<forge_server::ServerHandle>,
    pub speak: Option<forge_speak_queue::SpeakQueueHandle>,
    pub pipeline_config: Option<forge_speak_queue::PipelineConfigHandle>,
    pub bot_accounts: forge_types::Shared<Vec<String>>,
    pub speak_events: Option<forge_speak_queue::SpeakEventStream>,
    pub tts_registry: Option<Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>>,
    pub speech_output: Arc<forge_audio::DeviceSink>,
    pub speech_sink: Arc<dyn forge_audio::AudioSink>,
    pub audio_router: Arc<AudioRouter>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<crate::hotkey_sync::HotkeyReconciler>>,
    pub soundboard_player: Arc<forge_soundboard::SoundboardPlayer>,
    pub voice_gate: Arc<VoiceGateOwner>,
    pub stay_awake: forge_awake::StayAwake,
    pub first_run: crate::first_run::FirstRun,
}
