use forge_storage::SettingDisclosure::{Verbatim, Withheld};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{Flag, Setting, SettingSpec};

pub const CORE_HTTP_ALLOW_LOCAL: Setting<Flag> = Setting::new(
    "core.http_allow_local",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

pub const ACTION_FAILURE_OS_NOTICE: Setting<Flag> = Setting::new(
    "notifications.action_failure_os_notice",
    Flag { default: true },
    Portable,
    Withheld,
);

pub const CONNECTION_LOSS_SOUND: Setting<Flag> = Setting::new(
    "notifications.connection_loss_sound",
    Flag { default: false },
    Portable,
    Withheld,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    CORE_HTTP_ALLOW_LOCAL.spec(),
    ACTION_FAILURE_OS_NOTICE.spec(),
    CONNECTION_LOSS_SOUND.spec(),
];

#[cfg(test)]
mod tests {
    use forge_storage::SettingsCatalog;

    use super::SETTING_SPECS;

    #[test]
    fn declared_settings_match_the_core_catalog_rows_they_duplicate() {
        let core = SettingsCatalog::core();
        for spec in SETTING_SPECS {
            let core_row = core
                .specs()
                .iter()
                .find(|row| row.key.name() == spec.key.name());

            assert_eq!(core_row, Some(spec), "{}", spec.key.name());
        }
    }
}
