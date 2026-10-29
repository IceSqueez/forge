use forge_storage::SettingDisclosure::{Kind, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{Count, Flag, Setting, SettingEncoding, SettingSpec};

const BYTES_PER_KIB: u32 = 1024;

pub const OP_LIMIT: Setting<Count> = Setting::new(
    "script.op_limit",
    Count {
        default: 100_000,
        min: 1_000,
        max: 10_000_000,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const TIMEOUT_MS: Setting<Count> = Setting::new(
    "script.timeout_ms",
    Count {
        default: 500,
        min: 50,
        max: 10_000,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const HTTP_MAX_CALLS: Setting<Count> = Setting::new(
    "script.http_max_calls_per_script",
    Count {
        default: 10,
        min: 1,
        max: 100,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const HTTP_TIMEOUT_MS: Setting<Count> = Setting::new(
    "script.http_timeout_ms",
    Count {
        default: 5_000,
        min: 100,
        max: 30_000,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const HTTP_MAX_RESPONSE_BYTES: Setting<Count> = Setting::new(
    "script.http_max_response_bytes",
    Count {
        default: 1_024 * BYTES_PER_KIB,
        min: BYTES_PER_KIB,
        max: 10_240 * BYTES_PER_KIB,
        unclamped: None,
    },
    Portable,
    Verbatim,
);

pub const HTTP_ALLOW_LOCAL: Setting<Flag> = Setting::new(
    "script.http_allow_local",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

pub const HTTP_ALLOWED_DOMAINS: SettingSpec = SettingSpec::exact(
    "script.http_allowed_domains",
    SettingEncoding::Json,
    MachineLocal,
    Kind,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    HTTP_ALLOWED_DOMAINS,
    HTTP_MAX_CALLS.spec(),
    HTTP_TIMEOUT_MS.spec(),
    HTTP_ALLOW_LOCAL.spec(),
    HTTP_MAX_RESPONSE_BYTES.spec(),
    OP_LIMIT.spec(),
    TIMEOUT_MS.spec(),
];
