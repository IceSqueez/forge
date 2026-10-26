use std::sync::Arc;

use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, ChatHistoryRetentionHandle, DateNamesHandle, EventBus, LatestValues,
    LiveViewerAggregatorHandle, NotificationPolicyHandle, OverlayServiceHandle,
    QueueSchedulerHandle, ScheduledRunsHandle, ScriptRegistry, TimerSchedulerHandle,
    TriggerEvaluatorHandle,
};
use forge_storage::{CredentialsKeyLoss, DataProvider, Language};

use crate::audio_router::AudioRouter;
use crate::integrations::{BuiltinObject, BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
use crate::log_tail::LogTail;
use crate::platform_credentials::PlatformCredentialManagers;
use crate::voice_gate::VoiceGateOwner;
use forge_runtime::IntegrationSupervisor;

pub struct RuntimeHandles {
    pub rt_handle: tokio::runtime::Handle,
    pub log_tail: LogTail,
    pub backend: Arc<dyn DataProvider>,
    pub startup_language: Language,
    pub startup_display_formats: forge_components::DisplayFormatChoices,
    pub anonymous_donor: forge_types::Shared<String>,
    pub date_names: DateNamesHandle,
    pub latest_values: LatestValues,
    pub credentials_key_loss: Option<CredentialsKeyLoss>,
    pub overlay_build_failure: Option<crate::overlay_build_failure::OverlayBuildFailure>,
    pub bus: Arc<EventBus>,
    pub script_registry: Arc<ScriptRegistry>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub overlay_kinds: Arc<OverlayKindRegistry>,
    pub overlays: OverlayServiceHandle,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub scheduled_runs: ScheduledRunsHandle,
    pub timer_scheduler: TimerSchedulerHandle,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub chat_history_retention: ChatHistoryRetentionHandle,
    pub notification_policy: NotificationPolicyHandle,
    pub transfer_exclusivity: forge_transfer::TransferExclusivity,
    pub scheduled_backups: forge_transfer::ScheduledBackupHandle,
    pub builtins: BuiltinRegistry,
    pub integrations: IntegrationSupervisor<BuiltinObject>,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Option<Arc<forge_discord::DiscordClient>>,
    pub donation_services: crate::donation_services::DonationServices,
    pub twitch_bot: Option<Arc<forge_platform_twitch::TwitchBotAccount>>,
    pub twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub youtube_sign_in: Option<forge_platform_youtube::YoutubeSignIn>,
    pub kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub platform_credentials: PlatformCredentialManagers,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub server: Option<forge_server::ServerHandle>,
    pub server_unavailable: Option<String>,
    pub speak: Option<forge_speak_queue::SpeakQueueHandle>,
    pub pipeline_config: Option<forge_speak_queue::PipelineConfigHandle>,
    pub bot_accounts: forge_types::Shared<Vec<String>>,
    pub speak_events: Option<forge_speak_queue::SpeakEventStream>,
    pub tts_registry: Option<Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>>,
    pub cloud_tts_gateways: forge_tts_cloud::CloudGateways,
    pub speech_output: Arc<forge_audio::DeviceSink>,
    pub audio_router: Arc<AudioRouter>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Option<Arc<forge_hotkey::HotkeyReconciler>>,
    pub soundboard_player: Arc<forge_soundboard::SoundboardPlayer>,
    pub voice_gate: Arc<VoiceGateOwner>,
    pub stay_awake: forge_awake::StayAwake,
    pub first_run: crate::first_run::FirstRun,
}
