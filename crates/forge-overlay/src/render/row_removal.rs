use forge_types::{ChatModerationAction, PlatformId};
use serde_json::{Value, json};

use super::page_contract::REMOVE_FRAME;

const SCOPE_MESSAGE: &str = "message";
const SCOPE_USER: &str = "user";
const SCOPE_PLATFORM: &str = "platform";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowRemovalScope {
    Message { message_id: String },
    User { user_id: String },
    Platform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowRemoval {
    pub platform: PlatformId,
    pub scope: RowRemovalScope,
}

impl RowRemoval {
    pub fn from_moderation(platform: PlatformId, action: &ChatModerationAction) -> Option<Self> {
        let scope = match action {
            ChatModerationAction::DeleteMessage { message_id } if !message_id.is_empty() => {
                RowRemovalScope::Message {
                    message_id: message_id.clone(),
                }
            }
            ChatModerationAction::RemoveUser {
                user_id: Some(user_id),
                ..
            } if !user_id.is_empty() => RowRemovalScope::User {
                user_id: user_id.clone(),
            },
            ChatModerationAction::ClearChat => RowRemovalScope::Platform,
            ChatModerationAction::DeleteMessage { .. }
            | ChatModerationAction::RemoveUser { .. } => return None,
        };
        Some(Self { platform, scope })
    }

    pub fn frame(&self) -> Value {
        let platform = self.platform.as_str();
        match &self.scope {
            RowRemovalScope::Message { message_id } => json!({
                "frame": REMOVE_FRAME,
                "platform": platform,
                "scope": SCOPE_MESSAGE,
                "messageId": message_id,
            }),
            RowRemovalScope::User { user_id } => json!({
                "frame": REMOVE_FRAME,
                "platform": platform,
                "scope": SCOPE_USER,
                "userId": user_id,
            }),
            RowRemovalScope::Platform => json!({
                "frame": REMOVE_FRAME,
                "platform": platform,
                "scope": SCOPE_PLATFORM,
            }),
        }
    }
}
