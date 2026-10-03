use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::crowd::Crowd;
use super::expectation::Expectation;
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage {
    pub viewer: ChatViewer,
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
            Self::ObsOnline {} => "obs_online",
            Self::ObsIdentified { .. } => "obs_identified",
            Self::ObsRestart { .. } => "obs_restart",
            Self::ObsSceneSwitch { .. } => "obs_scene_switch",
            Self::ObsStream { .. } => "obs_stream",
            Self::ObsInputMute { .. } => "obs_input_mute",
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
}
