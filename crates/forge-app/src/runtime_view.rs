use std::sync::Arc;
use std::time::SystemTime;

use forge_discord::DiscordClient;
use forge_hotkey::HotkeyClient;
use forge_midi::MidiClient;
use forge_obs::{ObsClient, SwitchableObsSink};
use forge_platform_kick::KickAuthFlow;
use forge_platform_twitch::{ChatSendBridgeHandle, TwitchIntegrationBundle};
use forge_platform_youtube::GoogleAuthFlow;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, EventBus, QueueSchedulerHandle, ScriptRegistry, actions::ActionsService,
};
use forge_soundboard::SoundboardPlayer;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::DataProvider;
use forge_tts_core::EngineId;
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
    pub sound_player: Option<Arc<SoundboardPlayer>>,
    pub twitch_builtin: Option<Arc<TwitchIntegrationBundle>>,
    pub chat_send_bridge: Option<ChatSendBridgeHandle>,
    pub twitch_flow: Option<TwitchFlowHandle>,
    pub youtube_flow: Option<Arc<tokio::sync::Mutex<Option<GoogleAuthFlow>>>>,
    pub kick_flow: Option<Arc<tokio::sync::Mutex<Option<KickAuthFlow>>>>,
    pub twitch_login: Option<String>,
    pub twitch_token_expires: Option<SystemTime>,
    pub twitch_reauth_required: bool,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
    pub tts_engine_ids: Vec<EngineId>,
}
