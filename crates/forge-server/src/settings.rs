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

pub const OVERLAY_ROOT: SettingSpec = SettingSpec::exact(
    "server.overlay_root",
    SettingEncoding::Text,
    MachineLocal,
    Presence,
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
    OVERLAY_ROOT,
    ADDITIONAL_ORIGINS,
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
