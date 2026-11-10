use std::sync::Arc;

use forge_net::TaskOwner;
use forge_overlay::OverlayKindRegistry;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, ActionsService, ChatHistoryRetentionHandle, EventBus,
    FollowKnowledgeHandle, IntegrationSupervisor, LatestValues, LifecycleStates,
    LiveViewerAggregatorHandle, MaterializePass, NotificationPolicyHandle, OverlaySampleSource,
    OverlayServiceHandle, QueueSchedulerHandle, RuntimeError, ScheduledRunsHandle, ScriptCatalog,
    TimerSchedulerHandle, TriggerEvaluatorHandle,
};
use forge_server::ServerHandle;
use forge_transfer::{ScheduledBackupHandle, TransferExclusivity};
use forge_types::Shared;

use crate::audio_router::AudioRouter;
use crate::boot_warnings::BootWarnings;
use crate::donation_services::DonationServices;
use crate::integration_sign_out::IntegrationSignOut;
use crate::integrations::{BuiltinObject, BuiltinRegistry, ObsInstallSeed, VTubeInstallSeed};
use crate::server_boot::ServerUnavailable;
use crate::stop::OrderedStop;
use crate::voice_gate::VoiceGateOwner;

pub struct CoreHandles {
    pub bus: Arc<EventBus>,
    pub services: TaskOwner,
    pub latest_values: LatestValues,
    pub notification_policy: NotificationPolicyHandle,
    pub chat_history_retention: ChatHistoryRetentionHandle,
    pub transfer_exclusivity: TransferExclusivity,
    pub scheduled_backups: ScheduledBackupHandle,
}

pub struct AutomationHandles {
    pub script_catalog: Arc<ScriptCatalog>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub scheduled_runs: ScheduledRunsHandle,
    pub timer_scheduler: TimerSchedulerHandle,
    pub live_viewers: LiveViewerAggregatorHandle,
    pub follow_knowledge: FollowKnowledgeHandle,
    pub bot_accounts: Shared<Vec<String>>,
}

pub struct IntegrationHandles {
    pub supervisor: IntegrationSupervisor<BuiltinObject>,
    pub builtins: BuiltinRegistry,
    pub obs_install_seed: ObsInstallSeed,
    pub vtube_install_seed: VTubeInstallSeed,
    pub discord_client: Option<Arc<forge_discord::DiscordClient>>,
    pub midi_sink: Arc<forge_midi::SwitchableMidiSink>,
    pub hotkey_client: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub hotkey_reconciler: Arc<forge_hotkey::HotkeyReconciler>,
    pub donation_services: DonationServices,
    pub twitch_bot: Option<Arc<forge_platform_twitch::TwitchBotAccount>>,
    pub twitch_sign_in: Option<forge_platform_twitch::TwitchSignIn>,
    pub youtube_sign_in: Option<forge_platform_youtube::YouTubeSignIn>,
    pub kick_sign_in: Option<forge_platform_kick::KickSignIn>,
    pub sign_out: IntegrationSignOut,
}

pub struct SpeechHandles {
    pub speak: Option<forge_speak_queue::SpeakQueueHandle>,
    pub speak_events: Option<forge_speak_queue::SpeakEventStream>,
    pub pipeline_config: Option<forge_speak_queue::PipelineConfigHandle>,
    pub cloud_tts_gateways: forge_tts_cloud::CloudGateways,
    pub speech_output: Arc<forge_audio::DeviceSink>,
    pub voice_gate: Arc<VoiceGateOwner>,
}

pub struct AudioHandles {
    pub audio_router: Arc<AudioRouter>,
    pub soundboard_player: Arc<forge_soundboard::SoundboardPlayer>,
}

pub struct OverlayHandles {
    pub service: OverlayServiceHandle,
    pub samples: Arc<dyn OverlaySampleSource>,
    pub actions: Arc<ActionsService>,
    pub kinds: Arc<OverlayKindRegistry>,
}

pub struct BootOutcomes {
    pub warnings: BootWarnings,
    pub server_unavailable: Option<ServerUnavailable>,
    pub overlay_pass: Result<MaterializePass, RuntimeError>,
    pub integration_states: LifecycleStates,
}

pub struct Host {
    pub core: CoreHandles,
    pub automation: AutomationHandles,
    pub integrations: IntegrationHandles,
    pub speech: SpeechHandles,
    pub audio: AudioHandles,
    pub overlays: OverlayHandles,
    pub server: Option<ServerHandle>,
    pub outcomes: BootOutcomes,
    pub stop: OrderedStop,
}
