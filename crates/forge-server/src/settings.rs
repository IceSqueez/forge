use forge_storage::SettingDisclosure::{Kind, Presence, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{
    Count, Flag, PlainText, SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT, Setting, SettingCodec,
    SettingEncoding, SettingSpec,
};

use crate::config::{DEFAULT_PORT, LOCALHOST_BIND_ADDRESS};

pub const ENABLED: Setting<Flag> =
    Setting::new("server.enabled", Flag { default: true }, Portable, Verbatim);

pub const BIND_ADDRESS: Setting<PlainText> = Setting::new(
    "server.bind_address",
    PlainText {
        default: LOCALHOST_BIND_ADDRESS,
    },
    MachineLocal,
    Presence,
);

pub const PORT: Setting<Count> = Setting::new(
    "server.port",
    Count {
        default: DEFAULT_PORT as u32,
        min: u16::MIN as u32,
        max: u16::MAX as u32,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredOrigins {
    pub origins: Vec<String>,
    pub unreadable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginsCodec;

impl SettingCodec for OriginsCodec {
    type Value = StoredOrigins;
    const ENCODING: SettingEncoding = SettingEncoding::Json;

    fn default_value(&self) -> StoredOrigins {
        StoredOrigins::default()
    }

    fn decode(&self, raw: &str) -> Option<StoredOrigins> {
        Some(match serde_json::from_str(raw) {
            Ok(origins) => StoredOrigins {
                origins,
                unreadable: false,
            },
            Err(_) => {
                tracing::warn!(
                    key = ADDITIONAL_ORIGINS.name(),
                    "stored additional origins cannot be decoded; serving the default origins only"
                );
                StoredOrigins {
                    origins: Vec::new(),
                    unreadable: true,
                }
            }
        })
    }

    fn encode(&self, value: &StoredOrigins) -> Option<String> {
        serde_json::to_string(&value.origins).ok()
    }
}

pub const ADDITIONAL_ORIGINS: Setting<OriginsCodec> = Setting::new(
    "server.additional_origins",
    OriginsCodec,
    MachineLocal,
    Kind,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    ENABLED.spec(),
    BIND_ADDRESS.spec(),
    PORT.spec(),
    LAN_BIND_ENABLED.spec(),
    AUTH_REQUIRED_FOR_READS.spec(),
    HTTP_OVERLAY_REQUIRE_TOKEN.spec(),
    OVERLAY_CORS_ANY_ORIGIN.spec(),
    ADDITIONAL_ORIGINS.spec(),
];
