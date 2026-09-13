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
    /// Waits until forge authenticates a control connection.
    ForgeReady {
        within_ms: u64,
    },
    /// Waits until the fake Twitch holds a subscription of every listed type.
    TwitchSubscribed {
        types: Vec<String>,
        within_ms: u64,
    },
    Chat(ChatMessage),
    Crowd(Crowd),
    /// Injects `session_reconnect`, then waits until forge opens the successor session.
    SessionReconnect {
        within_ms: u64,
    },
    /// A fixed wait; `reason` is mandatory because an event-driven wait is almost always better.
    Pause {
        ms: u64,
        reason: String,
    },
    /// Runs a fixture action, resolved to its id by name.
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage {
    pub viewer: ChatViewer,
    pub text: String,
}

/// `display_name` falls back to `login`.
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
            Self::SessionReconnect { .. } => "session_reconnect",
            Self::Pause { .. } => "pause",
            Self::RunAction { .. } => "run_action",
            Self::SetGlobal { .. } => "set_global",
        }
    }

    pub fn needs_fake_twitch(&self) -> bool {
        matches!(
            self,
            Self::TwitchSubscribed { .. }
                | Self::Chat(_)
                | Self::Crowd(_)
                | Self::SessionReconnect { .. }
        )
    }
}
