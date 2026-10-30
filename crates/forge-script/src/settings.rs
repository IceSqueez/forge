use forge_storage::SettingDisclosure::{Kind, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::{Count, Flag, Setting, SettingCodec, SettingEncoding, SettingSpec};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedDomains;

impl AllowedDomains {
    fn parse_comma_separated(raw: &str) -> Vec<String> {
        raw.split(',')
            .map(str::trim)
            .filter(|domain| !domain.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

impl SettingCodec for AllowedDomains {
    type Value = Vec<String>;
    const ENCODING: SettingEncoding = SettingEncoding::Json;

    fn default_value(&self) -> Vec<String> {
        Vec::new()
    }

    fn decode(&self, raw: &str) -> Option<Vec<String>> {
        Some(serde_json::from_str(raw).unwrap_or_else(|_| Self::parse_comma_separated(raw)))
    }

    fn encode(&self, value: &Vec<String>) -> Option<String> {
        serde_json::to_string(value).ok()
    }
}

pub const HTTP_ALLOWED_DOMAINS: Setting<AllowedDomains> = Setting::new(
    "script.http_allowed_domains",
    AllowedDomains,
    MachineLocal,
    Kind,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    HTTP_ALLOWED_DOMAINS.spec(),
    HTTP_MAX_CALLS.spec(),
    HTTP_TIMEOUT_MS.spec(),
    HTTP_ALLOW_LOCAL.spec(),
    HTTP_MAX_RESPONSE_BYTES.spec(),
    OP_LIMIT.spec(),
    TIMEOUT_MS.spec(),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_domains_decode_json_then_legacy_csv_and_never_yield_blank_entries() {
        for (raw, expected) in [
            (r#"["a.com","b.com"]"#, vec!["a.com", "b.com"]),
            ("[]", vec![]),
            ("a.com, b.com ,c.com,", vec!["a.com", "b.com", "c.com"]),
            ("solo.com", vec!["solo.com"]),
            ("", vec![]),
            (" , ,, ", vec![]),
            (r#"["a.com""#, vec![r#"["a.com""#]),
            (r#"{"a":1}"#, vec![r#"{"a":1}"#]),
            ("42", vec!["42"]),
        ] {
            assert_eq!(
                AllowedDomains.decode(raw),
                Some(expected.iter().map(|d| (*d).to_owned()).collect()),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn allowed_domains_survive_an_encode_decode_round_trip() {
        let domains = vec!["a.com".to_owned(), "with,comma.example".to_owned()];

        let encoded = AllowedDomains.encode(&domains).unwrap();

        assert_eq!(AllowedDomains.decode(&encoded), Some(domains));
    }
}
