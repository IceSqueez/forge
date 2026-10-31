use forge_storage::SettingDisclosure::Withheld;
use forge_storage::SettingPortability::Portable;
use forge_storage::{Flag, Setting};

pub const OBS_AUTO_RECONNECT: Setting<Flag> = Setting::new(
    "obs.auto_reconnect",
    Flag { default: true },
    Portable,
    Withheld,
);

pub const OBS_CONNECT_ON_LAUNCH: Setting<Flag> = Setting::new(
    "obs.connect_on_launch",
    Flag { default: true },
    Portable,
    Withheld,
);
