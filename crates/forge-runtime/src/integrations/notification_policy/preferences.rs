use forge_storage::{SettingsRepo, read_setting_or_default};

use super::intent::NotificationIntent;
use crate::settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationPreferences {
    pub action_failure_notice: bool,
    pub connection_loss_cue: bool,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            action_failure_notice: settings::ACTION_FAILURE_OS_NOTICE.codec().default,
            connection_loss_cue: settings::CONNECTION_LOSS_SOUND.codec().default,
        }
    }
}

impl NotificationPreferences {
    pub async fn load(repo: &dyn SettingsRepo) -> Self {
        Self {
            action_failure_notice: read_setting_or_default(
                repo,
                &settings::ACTION_FAILURE_OS_NOTICE,
            )
            .await,
            connection_loss_cue: read_setting_or_default(repo, &settings::CONNECTION_LOSS_SOUND)
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
