use forge_platform_core::{ViewerAction, ViewerRef, ViewerStep};
use forge_types::Variant;

use crate::sub_actions::MAX_TIMEOUT_MINUTES;
use crate::vocabulary::{config_keys, sub_action_ids};

const SECONDS_PER_MINUTE: u64 = 60;

pub fn viewer_step(viewer: ViewerRef<'_>, action: &ViewerAction) -> Option<ViewerStep> {
    let user_id = viewer.viewer_id.filter(|id| id.parse::<u64>().is_ok())?;
    let user = (
        config_keys::USER_ID.to_owned(),
        Variant::String(user_id.to_owned()),
    );
    let display_name = viewer.display_name_entry(config_keys::DISPLAY_NAME);
    match action {
        ViewerAction::Timeout { seconds } => {
            let minutes = u64::try_from(*seconds)
                .ok()
                .filter(|seconds| *seconds >= 1)?
                .div_ceil(SECONDS_PER_MINUTE);
            let minutes = i64::try_from(minutes)
                .ok()
                .filter(|minutes| *minutes <= MAX_TIMEOUT_MINUTES)?;
            Some(ViewerStep::new(
                sub_action_ids::MODERATION_TIMEOUT,
                [
                    user,
                    display_name,
                    (
                        config_keys::DURATION_MINUTES.to_owned(),
                        Variant::Int(minutes),
                    ),
                ],
            ))
        }
        ViewerAction::Ban => Some(ViewerStep::new(
            sub_action_ids::MODERATION_BAN,
            [user, display_name],
        )),
        ViewerAction::Unban => Some(ViewerStep::new(sub_action_ids::MODERATION_UNBAN, [user])),
        ViewerAction::Shoutout | ViewerAction::Whisper(_) => None,
    }
}
