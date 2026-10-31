use forge_storage::SettingDisclosure::Withheld;
use forge_storage::SettingPortability::Portable;
use forge_storage::{Flag, Setting};

pub const VTUBE_AUTO_RECONNECT: Setting<Flag> = Setting::new(
    "vtube.auto_reconnect",
    Flag { default: true },
    Portable,
    Withheld,
);

pub const VTUBE_CONNECT_ON_LAUNCH: Setting<Flag> = Setting::new(
    "vtube.connect_on_launch",
    Flag { default: true },
    Portable,
    Withheld,
);
