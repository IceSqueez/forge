use std::sync::Arc;

use forge_awake::{StatusWatch, StayAwake, StayAwakeConfig};
use forge_storage::{SettingsRepo, get_bool_setting, reserved_keys, set_bool_setting};
use gpui::Context;

use crate::async_bridge::{SnapshotWriter, WriteOutcome};

pub(crate) const ENABLED_BY_DEFAULT: bool = true;
const HOLD_REASON: &str = "forge keeps the system and display awake while it runs";

#[derive(Clone)]
pub struct StayAwakeSetting {
    service: StayAwake,
    settings: Arc<dyn SettingsRepo>,
    writer: Arc<SnapshotWriter>,
}

impl StayAwakeSetting {
    pub fn watch(&self) -> StatusWatch {
        self.service.watch()
    }

    pub fn release(&self) {
        self.service.release();
    }

    pub fn set_enabled<V: 'static>(
        &self,
        enabled: bool,
        on_settled: impl FnOnce(&mut V, WriteOutcome, &mut Context<V>) + 'static,
        cx: &mut Context<V>,
    ) {
        let settings = Arc::clone(&self.settings);
        let service = self.service.clone();
        self.writer.write(
            async move {
                set_bool_setting(settings.as_ref(), reserved_keys::STAY_AWAKE, enabled)
                    .await
                    .map_err(|e| e.to_string())?;
                service.set_enabled(enabled).map_err(|e| e.to_string())
            },
            on_settled,
            cx,
        );
    }
}

pub async fn start_stay_awake(settings: Arc<dyn SettingsRepo>) -> StayAwakeSetting {
    let enabled = get_bool_setting(
        settings.as_ref(),
        reserved_keys::STAY_AWAKE,
        ENABLED_BY_DEFAULT,
    )
    .await;
    let service = StayAwake::spawn(StayAwakeConfig {
        enabled,
        reason: HOLD_REASON.to_owned(),
    });
    StayAwakeSetting {
        service,
        settings,
        writer: Arc::new(SnapshotWriter::new(&tokio::runtime::Handle::current())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_awake::EnabledPreference;
    use forge_storage::{get_bool_setting, reserved_keys};

    use super::{ENABLED_BY_DEFAULT, SettingsPreference};
    use crate::test_support::test_backend;

    #[tokio::test]
    async fn stored_opt_out_overrides_the_default_on_next_start() {
        let (settings, _writes) = test_backend();
        let preference = SettingsPreference(Arc::clone(&settings) as _);

        preference.store(false).await;

        assert!(
            !get_bool_setting(
                settings.as_ref(),
                reserved_keys::STAY_AWAKE,
                ENABLED_BY_DEFAULT
            )
            .await
        );
    }
}
