use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;

use forge_discord::DiscordClient;
use forge_hotkey::HotkeyClient;
use forge_midi::MidiClient;
use forge_obs::{ObsClient, SwitchableObsSink};
use forge_platform_core::ConnectionState;
use forge_platform_kick::{KickAuthFlow, KickIntegrationBundle};
use forge_platform_twitch::TwitchIntegrationBundle;
use forge_platform_youtube::GoogleAuthFlow;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, EventBus, QueueSchedulerHandle, ScriptRegistry, TtsTriggerSettingsHandle,
    actions::ActionsService,
};
use forge_soundboard::SoundboardPlayer;
use forge_speak_queue::{PipelineConfigHandle, SpeakQueueHandle};
use forge_storage::DataProvider;
use forge_tts_core::EngineId;
use forge_types::PlatformId;
use forge_vtube::{SwitchableVTubeSink, VTubeClient};

use crate::server_subsystem::ServerSubsystem;
use crate::twitch_panel::TwitchFlowHandle;

pub struct RuntimeView {
    pub backend: Arc<dyn DataProvider>,
    pub actions: Arc<ActionsService>,
    pub bus: Arc<EventBus>,
    pub script_registry: Arc<ScriptRegistry>,
    pub server_subsystem: Arc<ServerSubsystem>,
    pub action_engine: Option<ActionEngineHandle>,
    pub scheduler: Option<QueueSchedulerHandle>,
    pub obs_client: Option<Arc<ObsClient>>,
    pub obs_sink: Arc<SwitchableObsSink>,
    pub vtube_client: Option<Arc<VTubeClient>>,
    pub vtube_sink: Arc<SwitchableVTubeSink>,
    pub discord_client: Option<Arc<DiscordClient>>,
    pub midi_client: Option<Arc<MidiClient>>,
    pub hotkey_client: Option<Arc<HotkeyClient>>,
    pub speak_queue: Option<Arc<SpeakQueueHandle>>,
    /// `None` when the runtime is offline; the Filters screen's save then skips the live swap.
    pub pipeline_config: Option<PipelineConfigHandle>,
    /// `None` when the runtime is offline; the TTS Triggers screen's save then
    /// skips the live swap and defers gating changes to next boot.
    pub tts_trigger_settings: Option<TtsTriggerSettingsHandle>,
    pub sound_player: Option<Arc<SoundboardPlayer>>,
    pub twitch_builtin: Option<Arc<TwitchIntegrationBundle>>,
    pub kick_builtin: Option<Arc<KickIntegrationBundle>>,
    /// Latest per-platform connection state, fed by `CONNECTION_STATE_CHANGED_KIND`
    /// bus events. The connectivity indicator reads this instead of holding a
    /// `ChatPlatform` handle (Invariant #3).
    pub platform_connection: BTreeMap<PlatformId, ConnectionState>,
    pub twitch_flow: Option<TwitchFlowHandle>,
    pub youtube_flow: Option<Arc<tokio::sync::Mutex<Option<GoogleAuthFlow>>>>,
    pub kick_flow: Option<Arc<tokio::sync::Mutex<Option<KickAuthFlow>>>>,
    pub twitch_login: Option<String>,
    pub twitch_token_expires: Option<SystemTime>,
    pub twitch_reauth_required: bool,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub tts_engine_ids: Vec<EngineId>,
    /// `None` when the runtime is offline; the Cloud Engines save flow then
    /// only persists credentials and defers live registration to next boot.
    pub tts_registry: Option<Arc<std::sync::RwLock<forge_tts_core::TtsRegistry>>>,
}
