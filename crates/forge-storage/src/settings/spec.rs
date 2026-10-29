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
