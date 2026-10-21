use forge_storage::settings::{
    DEFAULT_NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE, DEFAULT_NOTIFICATIONS_CONNECTION_LOSS_SOUND,
};
use forge_storage::{SettingsRepo, get_bool_setting, reserved_keys};

use super::intent::NotificationIntent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationPreferences {
    pub action_failure_notice: bool,
    pub connection_loss_cue: bool,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            action_failure_notice: DEFAULT_NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE,
            connection_loss_cue: DEFAULT_NOTIFICATIONS_CONNECTION_LOSS_SOUND,
        }
    }
}

impl NotificationPreferences {
    pub async fn load(settings: &dyn SettingsRepo) -> Self {
        Self {
            action_failure_notice: get_bool_setting(
                settings,
                reserved_keys::NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE,
                DEFAULT_NOTIFICATIONS_ACTION_FAILURE_OS_NOTICE,
            )
            .await,
            connection_loss_cue: get_bool_setting(
                settings,
                reserved_keys::NOTIFICATIONS_CONNECTION_LOSS_SOUND,
                DEFAULT_NOTIFICATIONS_CONNECTION_LOSS_SOUND,
            )
            .await,
        }
    }

    pub(super) fn allows(&self, intent: &NotificationIntent) -> bool {
        match intent {
            NotificationIntent::ActionFailed(_)
            | NotificationIntent::ActionFailuresFolded { .. } => self.action_failure_notice,
            NotificationIntent::ConnectionLossCue { .. } => self.connection_loss_cue,
        }
    }
}
