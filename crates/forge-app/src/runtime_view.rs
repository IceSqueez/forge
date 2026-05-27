use std::sync::Arc;
use std::time::SystemTime;

use forge_obs::ObsClient;
use forge_platform_twitch::{ChatSendBridgeHandle, TwitchChatHandle};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::{
    ActionEngineHandle, CommandParserHandle, EventBus, QueueSchedulerHandle, ScriptRegistry,
    actions::ActionsService,
};
use forge_soundboard::SoundboardPlayer;
use forge_speak_queue::SpeakQueueHandle;
use forge_storage::DataProvider;

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
    pub command_parser: Option<CommandParserHandle>,
    pub obs_client: Option<Arc<ObsClient>>,
    pub speak_queue: Option<Arc<SpeakQueueHandle>>,
    pub sound_player: Option<Arc<SoundboardPlayer>>,
    pub twitch_chat_handle: Option<TwitchChatHandle>,
    pub chat_send_bridge: Option<ChatSendBridgeHandle>,
    pub twitch_flow: Option<TwitchFlowHandle>,
    pub twitch_login: Option<String>,
    pub twitch_token_expires: Option<SystemTime>,
    pub twitch_reauth_required: bool,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub trigger_registry: Arc<TriggerRegistry>,
}
