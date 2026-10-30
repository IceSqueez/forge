use forge_storage::SettingDisclosure::{Kind, Presence, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{
    Flag, SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT, Setting, SettingEncoding, SettingSpec,
};

pub const ENABLED: Setting<Flag> =
    Setting::new("server.enabled", Flag { default: true }, Portable, Verbatim);

pub const BIND_ADDRESS: SettingSpec = SettingSpec::exact(
    "server.bind_address",
    SettingEncoding::Text,
    MachineLocal,
    Presence,
);

pub const PORT: SettingSpec =
    SettingSpec::exact("server.port", SettingEncoding::Integer, Portable, Verbatim);

pub const LAN_BIND_ENABLED: Setting<Flag> = Setting::new(
    "server.lan_bind_enabled",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

pub const AUTH_REQUIRED_FOR_READS: Setting<Flag> = Setting::new(
    "server.auth_required_for_reads",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

pub const HTTP_OVERLAY_REQUIRE_TOKEN: Setting<Flag> = Setting::new(
    "server.http_overlay_require_token",
    Flag {
        default: SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
    },
    MachineLocal,
    Verbatim,
);

pub const OVERLAY_CORS_ANY_ORIGIN: Setting<Flag> = Setting::new(
    "server.overlay_cors_any_origin",
    Flag { default: true },
    MachineLocal,
    Verbatim,
);

pub const ADDITIONAL_ORIGINS: SettingSpec = SettingSpec::exact(
    "server.additional_origins",
    SettingEncoding::Json,
    MachineLocal,
    Kind,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    ENABLED.spec(),
    BIND_ADDRESS,
    PORT,
    LAN_BIND_ENABLED.spec(),
    AUTH_REQUIRED_FOR_READS.spec(),
    HTTP_OVERLAY_REQUIRE_TOKEN.spec(),
    OVERLAY_CORS_ANY_ORIGIN.spec(),
    ADDITIONAL_ORIGINS,
];
