use forge_storage::SettingDisclosure::{Kind, Verbatim, Withheld};
use forge_storage::SettingEncoding::{Flag, Integer, Json, Text};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{SettingSpec, SettingsCatalog, reserved_keys as key};

use crate::first_run::FIRST_RUN_COMPLETED_KEY;

pub const SETTING_SPECS: &[SettingSpec] = &[
    SettingSpec::exact(key::DENSITY, Text, Portable, Verbatim),
    SettingSpec::exact(key::KEYBOARD_SHORTCUTS, Json, Portable, Kind),
    SettingSpec::exact(key::AUDIO_SPEECH_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(key::AUDIO_CLIPS_ROUTE, Text, Portable, Verbatim),
    SettingSpec::exact(FIRST_RUN_COMPLETED_KEY, Flag, MachineLocal, Verbatim),
    SettingSpec::exact(key::STAY_AWAKE, Flag, MachineLocal, Verbatim),
    SettingSpec::exact(key::WINDOW_PLACEMENT, Json, MachineLocal, Kind),
    SettingSpec::exact(
        key::UPDATES_ROLLED_BACK_VERSION,
        Text,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(
        key::UPDATES_LEFTOVER_INSTALL_NOTICED,
        Flag,
        MachineLocal,
        Verbatim,
    ),
    SettingSpec::exact(key::OBS_AUTO_RECONNECT, Flag, Portable, Withheld),
    SettingSpec::exact(key::OBS_CONNECT_ON_LAUNCH, Flag, Portable, Withheld),
    SettingSpec::exact(key::VTUBE_AUTO_RECONNECT, Flag, Portable, Withheld),
    SettingSpec::exact(key::VTUBE_CONNECT_ON_LAUNCH, Flag, Portable, Withheld),
    SettingSpec::exact(key::MIDI_KNOWN_DEVICES, Json, MachineLocal, Withheld),
    SettingSpec::exact(
        key::DONATELLO_POLL_INTERVAL_SECS,
        Integer,
        Portable,
        Withheld,
    ),
    SettingSpec::exact(
        key::NOTIFICATIONS_SCHEDULED_BACKUP_FAILURE_OS_NOTICE,
        Flag,
        Portable,
        Withheld,
    ),
];

const OWNER_SPECS: &[&[SettingSpec]] = &[
    forge_script::settings::SETTING_SPECS,
    forge_soundboard::settings::SETTING_SPECS,
    forge_hotkey::SETTING_SPECS,
    forge_server::settings::SETTING_SPECS,
    forge_runtime::settings::SETTING_SPECS,
    SETTING_SPECS,
];

pub fn registered_settings_catalog() -> SettingsCatalog {
    let mut catalog = SettingsCatalog::core();
    for specs in OWNER_SPECS {
        if let Err(error) = catalog.register(specs) {
            tracing::error!(%error, "owner settings specs left unregistered");
        }
    }
    catalog
}
