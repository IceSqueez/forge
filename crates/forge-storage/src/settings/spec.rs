use super::decode_bool_setting;
use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingKey {
    Exact(&'static str),
    Prefix(&'static str),
}

impl SettingKey {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exact(name) | Self::Prefix(name) => name,
        }
    }

    pub fn matches(self, stored_key: &str) -> bool {
        match self {
            Self::Exact(name) => name == stored_key,
            Self::Prefix(prefix) => stored_key.starts_with(prefix),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingEncoding {
    Text,
    Flag,
    Integer,
    Decimal,
    Json,
}

impl SettingEncoding {
    pub fn accepts(self, raw: &str) -> bool {
        match self {
            Self::Text => true,
            Self::Flag => decode_bool_setting(raw).is_some(),
            Self::Integer => {
                let trimmed = raw.trim();
                trimmed.parse::<i64>().is_ok() || trimmed.parse::<u64>().is_ok()
            }
            Self::Decimal => raw.trim().parse::<f64>().is_ok_and(f64::is_finite),
            Self::Json => serde_json::from_str::<serde_json::Value>(raw).is_ok(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SettingSpec {
    pub key: SettingKey,
    pub encoding: SettingEncoding,
    pub portability: SettingPortability,
    pub disclosure: SettingDisclosure,
}

impl SettingSpec {
    pub const fn exact(
        name: &'static str,
        encoding: SettingEncoding,
        portability: SettingPortability,
        disclosure: SettingDisclosure,
    ) -> Self {
        Self {
            key: SettingKey::Exact(name),
            encoding,
            portability,
            disclosure,
        }
    }

    pub const fn prefix(
        prefix: &'static str,
        encoding: SettingEncoding,
        portability: SettingPortability,
        disclosure: SettingDisclosure,
    ) -> Self {
        Self {
            key: SettingKey::Prefix(prefix),
            encoding,
            portability,
            disclosure,
        }
    }
}
