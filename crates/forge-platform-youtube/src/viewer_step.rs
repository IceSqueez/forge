use forge_platform_core::{ViewerAction, ViewerRef, ViewerStep};
use forge_types::Variant;

use crate::sub_actions::MAX_TIMEOUT_SECONDS;
use crate::vocabulary::{config_keys, sub_action_ids};

pub fn viewer_step(viewer: ViewerRef<'_>, action: &ViewerAction) -> Option<ViewerStep> {
    let channel_id = viewer.viewer_id.filter(|id| !id.is_empty())?;
    let channel = (
        config_keys::CHANNEL_ID.to_owned(),
        Variant::String(channel_id.to_owned()),
    );
    let display_name = viewer.display_name_entry(config_keys::DISPLAY_NAME);
    match action {
        ViewerAction::Timeout { seconds } => {
            (1..=MAX_TIMEOUT_SECONDS).contains(seconds).then(|| {
                ViewerStep::new(
                    sub_action_ids::MODERATION_TIMEOUT_USER,
                    [
                        channel,
                        display_name,
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
            [channel, display_name],
        )),
        ViewerAction::Unban => Some(ViewerStep::new(
            sub_action_ids::MODERATION_UNBAN_USER,
            [channel],
        )),
        ViewerAction::Shoutout | ViewerAction::Whisper(_) => None,
    }
}
