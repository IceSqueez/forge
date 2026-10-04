use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::crowd::Crowd;
use super::donation::{DonatelloGift, MonobankGift, OfflineGift};
use super::expectation::Expectation;
use crate::kick::KickChatter;
use crate::twitch::{Viewer, ViewerBadge};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    #[serde(rename = "do")]
    pub action: StepAction,
    #[serde(default)]
    pub expect: Vec<Expectation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum StepAction {
    ForgeReady {
        within_ms: u64,
    },
    TwitchSubscribed {
        types: Vec<String>,
        within_ms: u64,
    },
    Chat(ChatMessage),
    Crowd(Crowd),
    TwitchEvent {
        subscription_type: String,
        event: Value,
    },
    SessionReconnect {
        within_ms: u64,
    },
    OverlayPage {
        overlay: String,
        within_ms: u64,
    },
    Pause {
        ms: u64,
        reason: String,
    },
    RunAction {
        action: String,
        #[serde(default)]
        args: Map<String, Value>,
    },
    SetGlobal {
        name: String,
        value: Value,
        #[serde(default)]
        persisted: bool,
    },
    KickChatJoined {
        within_ms: u64,
    },
    KickChat(KickChatMessage),
    KickPusherEvent {
        event: String,
        data: Value,
    },
    KickStream {
        live: bool,
    },
    KickChannelPolled {
        within_ms: u64,
    },
    ObsOnline {},
    ObsIdentified {
        within_ms: u64,
    },
    ObsRestart {
        down_ms: u64,
    },
    ObsSceneSwitch {
        scene: String,
    },
    ObsStream {
        active: bool,
    },
    ObsInputMute {
        input: String,
        muted: bool,
    },
    VtubeOnline {},
    VtubeAuthenticated {
        within_ms: u64,
    },
    VtubeHotkey {
        hotkey: String,
    },
    VtubeModelLoad {
        model: String,
    },
    VtubeModelUnload {},
    VtubeModelConfigChanged {},
    VtubeTracking {
        face_found: bool,
    },
    VtubeItemAdded {
        file: String,
    },
    VtubeItemRemoved {
        file: String,
    },
    VtubeExpression {
        file: String,
        active: bool,
    },
    DonatelloDonation(DonatelloGift),
    MonobankTopUp(MonobankGift),
    DonationsPolled {
        within_ms: u64,
    },
    ForgeRestart {
        within_ms: u64,
        #[serde(default)]
        offline: Vec<OfflineGift>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage {
    pub viewer: ChatViewer,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickChatMessage {
    pub sender: KickChatter,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatViewer {
    pub user_id: String,
    pub login: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub badges: Vec<ViewerBadge>,
}

impl ChatViewer {
    pub fn to_viewer(&self) -> Viewer {
        let mut viewer = Viewer::new(self.user_id.clone(), self.login.clone());
        if let Some(display_name) = &self.display_name {
            viewer.display_name = display_name.clone();
        }
        viewer.badges = self.badges.clone();
        viewer
    }
}

impl StepAction {
    pub fn keyword(&self) -> &'static str {
        match self {
            Self::ForgeReady { .. } => "forge_ready",
            Self::TwitchSubscribed { .. } => "twitch_subscribed",
            Self::Chat(_) => "chat",
            Self::Crowd(_) => "crowd",
            Self::TwitchEvent { .. } => "twitch_event",
            Self::SessionReconnect { .. } => "session_reconnect",
            Self::OverlayPage { .. } => "overlay_page",
            Self::Pause { .. } => "pause",
            Self::RunAction { .. } => "run_action",
            Self::SetGlobal { .. } => "set_global",
            Self::KickChatJoined { .. } => "kick_chat_joined",
            Self::KickChat(_) => "kick_chat",
            Self::KickPusherEvent { .. } => "kick_pusher_event",
            Self::KickStream { .. } => "kick_stream",
            Self::KickChannelPolled { .. } => "kick_channel_polled",
            Self::ObsOnline {} => "obs_online",
            Self::ObsIdentified { .. } => "obs_identified",
            Self::ObsRestart { .. } => "obs_restart",
            Self::ObsSceneSwitch { .. } => "obs_scene_switch",
            Self::ObsStream { .. } => "obs_stream",
            Self::ObsInputMute { .. } => "obs_input_mute",
            Self::VtubeOnline {} => "vtube_online",
            Self::VtubeAuthenticated { .. } => "vtube_authenticated",
            Self::VtubeHotkey { .. } => "vtube_hotkey",
            Self::VtubeModelLoad { .. } => "vtube_model_load",
            Self::VtubeModelUnload {} => "vtube_model_unload",
            Self::VtubeModelConfigChanged {} => "vtube_model_config_changed",
            Self::VtubeTracking { .. } => "vtube_tracking",
            Self::VtubeItemAdded { .. } => "vtube_item_added",
            Self::VtubeItemRemoved { .. } => "vtube_item_removed",
            Self::VtubeExpression { .. } => "vtube_expression",
            Self::DonatelloDonation(_) => "donatello_donation",
            Self::MonobankTopUp(_) => "monobank_top_up",
            Self::DonationsPolled { .. } => "donations_polled",
            Self::ForgeRestart { .. } => "forge_restart",
        }
    }

    pub fn needs_fake_twitch(&self) -> bool {
        matches!(
            self,
            Self::TwitchSubscribed { .. }
                | Self::Chat(_)
                | Self::Crowd(_)
                | Self::TwitchEvent { .. }
                | Self::SessionReconnect { .. }
        )
    }

    pub fn needs_fake_kick(&self) -> bool {
        matches!(
            self,
            Self::KickChatJoined { .. }
                | Self::KickChat(_)
                | Self::KickPusherEvent { .. }
                | Self::KickStream { .. }
                | Self::KickChannelPolled { .. }
        )
    }

    pub fn needs_fake_obs(&self) -> bool {
        matches!(
            self,
            Self::ObsOnline {}
                | Self::ObsIdentified { .. }
                | Self::ObsRestart { .. }
                | Self::ObsSceneSwitch { .. }
                | Self::ObsStream { .. }
                | Self::ObsInputMute { .. }
        )
    }

    pub fn needs_fake_vtube(&self) -> bool {
        matches!(
            self,
            Self::VtubeOnline {}
                | Self::VtubeAuthenticated { .. }
                | Self::VtubeHotkey { .. }
                | Self::VtubeModelLoad { .. }
                | Self::VtubeModelUnload {}
                | Self::VtubeModelConfigChanged {}
                | Self::VtubeTracking { .. }
                | Self::VtubeItemAdded { .. }
                | Self::VtubeItemRemoved { .. }
                | Self::VtubeExpression { .. }
        )
    }
}
