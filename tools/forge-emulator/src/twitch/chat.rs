use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::config::FakeTwitchConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewerBadge {
    Broadcaster,
    Moderator,
    Vip,
    Subscriber { months: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: String,
    pub login: String,
    pub display_name: String,
    pub badges: Vec<ViewerBadge>,
}

impl Viewer {
    pub fn new(user_id: impl Into<String>, login: impl Into<String>) -> Self {
        let login = login.into();
        Self {
            user_id: user_id.into(),
            display_name: login.clone(),
            login,
            badges: Vec::new(),
        }
    }

    pub fn with_badge(mut self, badge: ViewerBadge) -> Self {
        self.badges.push(badge);
        self
    }

    pub(crate) fn user_json(&self) -> Value {
        user_json(&self.user_id, &self.login, &self.display_name)
    }
}

pub(crate) fn user_json(id: &str, login: &str, display_name: &str) -> Value {
    json!({
        "id": id,
        "login": login,
        "display_name": display_name,
        "type": "",
        "broadcaster_type": "",
        "description": "",
        "profile_image_url": "",
        "offline_image_url": "",
        "view_count": 0,
        "created_at": "2020-01-01T00:00:00Z",
    })
}

fn badge_json(badge: ViewerBadge) -> Value {
    let (set_id, id, info) = match badge {
        ViewerBadge::Broadcaster => ("broadcaster", "1".to_owned(), String::new()),
        ViewerBadge::Moderator => ("moderator", "1".to_owned(), String::new()),
        ViewerBadge::Vip => ("vip", "1".to_owned(), String::new()),
        ViewerBadge::Subscriber { months } => {
            ("subscriber", months.to_string(), months.to_string())
        }
    };
    json!({ "set_id": set_id, "id": id, "info": info })
}

pub(crate) fn chat_message_event(
    config: &FakeTwitchConfig,
    viewer: &Viewer,
    text: &str,
    message_id: &str,
) -> Value {
    json!({
        "broadcaster_user_id": config.broadcaster_user_id,
        "broadcaster_user_login": config.broadcaster_login,
        "broadcaster_user_name": config.broadcaster_login,
        "source_broadcaster_user_id": null,
        "source_broadcaster_user_login": null,
        "source_broadcaster_user_name": null,
        "chatter_user_id": viewer.user_id,
        "chatter_user_login": viewer.login,
        "chatter_user_name": viewer.display_name,
        "message_id": message_id,
        "source_message_id": null,
        "message": {
            "text": text,
            "fragments": [
                { "type": "text", "text": text, "cheermote": null, "emote": null, "mention": null }
            ],
        },
        "color": "",
        "badges": viewer.badges.iter().copied().map(badge_json).collect::<Vec<_>>(),
        "source_badges": null,
        "message_type": "text",
        "cheer": null,
        "reply": null,
        "channel_points_custom_reward_id": null,
        "channel_points_animation_id": null,
    })
}
