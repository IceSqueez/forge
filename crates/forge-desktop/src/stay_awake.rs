use std::sync::Arc;

use forge_awake::{EnabledPreference, StayAwake, StayAwakeConfig};
use forge_storage::{SettingsRepo, get_bool_setting, reserved_keys, set_bool_setting};

const ENABLED_BY_DEFAULT: bool = true;
const HOLD_REASON: &str = "forge keeps the system and display awake while it runs";

struct SettingsPreference(Arc<dyn SettingsRepo>);

#[async_trait::async_trait]
impl EnabledPreference for SettingsPreference {
    async fn store(&self, enabled: bool) {
        if let Err(e) = set_bool_setting(self.0.as_ref(), reserved_keys::STAY_AWAKE, enabled).await
        {
            tracing::warn!(error = %e, "could not persist the stay-awake setting");
        }
    }
}

pub async fn start_stay_awake(settings: Arc<dyn SettingsRepo>) -> StayAwake {
    let enabled = get_bool_setting(
        settings.as_ref(),
        reserved_keys::STAY_AWAKE,
        ENABLED_BY_DEFAULT,
    )
    .await;
    StayAwake::spawn(
        StayAwakeConfig {
            enabled,
            reason: HOLD_REASON.to_owned(),
        },
        Arc::new(SettingsPreference(settings)),
    )
}
