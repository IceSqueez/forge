use forge_platform_core::{ViewerAction, ViewerRef, ViewerStep};
use forge_types::Variant;

use crate::sub_actions::MAX_DURATION_SECONDS;
use crate::vocabulary::{config_keys, sub_action_ids};

pub fn viewer_step(viewer: ViewerRef<'_>, action: &ViewerAction) -> Option<ViewerStep> {
    let user_id = viewer
        .viewer_id
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let id = Variant::String(user_id.to_owned());
    let target = (config_keys::TARGET_USER_ID.to_owned(), id.clone());
    match action {
        ViewerAction::Shoutout => Some(ViewerStep::new(
            sub_action_ids::CHANNEL_SEND_SHOUTOUT,
            [(config_keys::TO_BROADCASTER_ID.to_owned(), id)],
        )),
        ViewerAction::Whisper(message) => Some(ViewerStep::new(
            sub_action_ids::CHAT_SEND_WHISPER,
            [
                (config_keys::TO_USER_ID.to_owned(), id),
                (
                    config_keys::MESSAGE.to_owned(),
                    Variant::String(message.clone()),
                ),
            ],
        )),
        ViewerAction::Timeout { seconds } => {
            (1..=MAX_DURATION_SECONDS).contains(seconds).then(|| {
                ViewerStep::new(
                    sub_action_ids::MODERATION_TIMEOUT_USER,
                    [
                        target,
                        (
                            config_keys::DURATION_SECONDS.to_owned(),
                            Variant::Int(*seconds),
                        ),
                    ],
                )
            })
        }
        ViewerAction::Ban => Some(ViewerStep::new(
            sub_action_ids::MODERATION_BAN_USER,
            [target],
        )),
        ViewerAction::Unban => Some(ViewerStep::new(
            sub_action_ids::MODERATION_UNBAN_USER,
            [target],
        )),
    }
}
