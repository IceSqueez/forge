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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::UserRemoval;
    use serde_json::json;

    fn remove_user(user_id: Option<&str>, removal: UserRemoval) -> ChatModerationAction {
        ChatModerationAction::RemoveUser {
            user_id: user_id.map(str::to_owned),
            user_name: "viewer".to_owned(),
            removal,
        }
    }

    #[test]
    fn a_deleted_message_becomes_a_message_frame_for_its_platform() {
        let removal = RowRemoval::from_moderation(
            PlatformId::Kick,
            &ChatModerationAction::DeleteMessage {
                message_id: "m-1".to_owned(),
            },
        )
        .expect("a delete names its message");

        assert_eq!(
            removal.frame(),
            json!({"frame": "remove", "platform": "kick", "scope": "message", "messageId": "m-1"})
        );
    }

    #[test]
    fn a_timeout_and_a_ban_both_remove_the_viewer_by_id() {
        for kind in [UserRemoval::Timeout, UserRemoval::Ban] {
            let removal =
                RowRemoval::from_moderation(PlatformId::YouTube, &remove_user(Some("UC1"), kind))
                    .expect("a removal with an id is mappable");

            assert_eq!(
                removal.frame(),
                json!({"frame": "remove", "platform": "youtube", "scope": "user", "userId": "UC1"})
            );
        }
    }

    #[test]
    fn a_cleared_chat_removes_the_whole_platform() {
        let removal =
            RowRemoval::from_moderation(PlatformId::Twitch, &ChatModerationAction::ClearChat)
                .expect("a clear needs no id");

        assert_eq!(
            removal.frame(),
            json!({"frame": "remove", "platform": "twitch", "scope": "platform"})
        );
    }

    #[test]
    fn an_action_naming_nothing_removable_maps_to_no_frame() {
        for action in [
            ChatModerationAction::DeleteMessage {
                message_id: String::new(),
            },
            remove_user(None, UserRemoval::Ban),
            remove_user(Some(""), UserRemoval::Timeout),
        ] {
            assert_eq!(
                RowRemoval::from_moderation(PlatformId::Twitch, &action),
                None
            );
        }
    }
}
