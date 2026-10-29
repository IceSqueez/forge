use forge_types::LogLevel;

use super::decode_bool_setting;
use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::spec::{SettingEncoding, SettingSpec};
use crate::{SettingsRepo, StorageError};

pub trait SettingCodec: Send + Sync {
    type Value: Send + Sync;
    const ENCODING: SettingEncoding;

    fn default_value(&self) -> Self::Value;
    fn decode(&self, raw: &str) -> Option<Self::Value>;
    fn encode(&self, value: &Self::Value) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flag {
    pub default: bool,
}

impl SettingCodec for Flag {
    type Value = bool;
    const ENCODING: SettingEncoding = SettingEncoding::Flag;

    fn default_value(&self) -> bool {
        self.default
    }

    fn decode(&self, raw: &str) -> Option<bool> {
        decode_bool_setting(raw)
    }

    fn encode(&self, value: &bool) -> Option<String> {
        Some(if *value { "true" } else { "false" }.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub default: u32,
    pub min: u32,
    pub max: u32,
    pub unclamped: Option<u32>,
}

impl Count {
    pub fn clamp(&self, value: u32) -> u32 {
        if self.unclamped == Some(value) {
            return value;
        }
        value.clamp(self.min, self.max)
    }
}

impl SettingCodec for Count {
    type Value = u32;
    const ENCODING: SettingEncoding = SettingEncoding::Integer;

    fn default_value(&self) -> u32 {
        self.default
    }

    fn decode(&self, raw: &str) -> Option<u32> {
        raw.trim().parse().ok().map(|value| self.clamp(value))
    }

    fn encode(&self, value: &u32) -> Option<String> {
        Some(self.clamp(*value).to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    pub default: f32,
    pub min: f32,
    pub max: f32,
}

impl Level {
    fn bounded(&self, value: f32) -> Option<f32> {
        value.is_finite().then(|| value.clamp(self.min, self.max))
    }
}

impl SettingCodec for Level {
    type Value = f32;
    const ENCODING: SettingEncoding = SettingEncoding::Decimal;

    fn default_value(&self) -> f32 {
        self.default
    }

    fn decode(&self, raw: &str) -> Option<f32> {
        raw.parse::<f32>()
            .ok()
            .and_then(|value| self.bounded(value))
    }

    fn encode(&self, value: &f32) -> Option<String> {
        self.bounded(*value).map(|value| value.to_string())
    }
}

const LOG_LEVELS: [LogLevel; 5] = [
    LogLevel::Trace,
    LogLevel::Debug,
    LogLevel::Info,
    LogLevel::Warn,
    LogLevel::Error,
];

pub fn log_level_as_str(level: &LogLevel) -> &'static str {
    match level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verbosity {
    pub default: LogLevel,
}

impl SettingCodec for Verbosity {
    type Value = LogLevel;
    const ENCODING: SettingEncoding = SettingEncoding::Text;

    fn default_value(&self) -> LogLevel {
        self.default.clone()
    }

    fn decode(&self, raw: &str) -> Option<LogLevel> {
        let raw = raw.trim();
        LOG_LEVELS
            .into_iter()
            .find(|level| raw.eq_ignore_ascii_case(log_level_as_str(level)))
    }

    fn encode(&self, value: &LogLevel) -> Option<String> {
        Some(log_level_as_str(value).to_owned())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Setting<C> {
    spec: SettingSpec,
    codec: C,
}

impl<C: SettingCodec> Setting<C> {
    pub const fn new(
        name: &'static str,
        codec: C,
        portability: SettingPortability,
        disclosure: SettingDisclosure,
    ) -> Self {
        Self {
            spec: SettingSpec::exact(name, C::ENCODING, portability, disclosure),
            codec,
        }
    }

    pub const fn spec(&self) -> SettingSpec {
        self.spec
    }

    pub const fn codec(&self) -> &C {
        &self.codec
    }

    pub const fn name(&self) -> &'static str {
        self.spec.key.name()
    }

    pub fn decode_or_default(&self, raw: Option<&str>) -> C::Value {
        let Some(raw) = raw else {
            return self.codec.default_value();
        };
        self.codec.decode(raw).unwrap_or_else(|| {
            tracing::warn!(
                key = self.name(),
                "stored setting cannot be decoded; using its default"
            );
            self.codec.default_value()
        })
    }
}

pub async fn read_setting<C: SettingCodec>(
    repo: &dyn SettingsRepo,
    setting: &Setting<C>,
) -> Result<C::Value, StorageError> {
    let raw = repo.get_string(setting.name()).await?;
    Ok(setting.decode_or_default(raw.as_deref()))
}

pub async fn write_setting<C: SettingCodec>(
    repo: &dyn SettingsRepo,
    setting: &Setting<C>,
    value: &C::Value,
) -> Result<(), StorageError> {
    let encoded = setting
        .codec
        .encode(value)
        .ok_or_else(|| StorageError::UnstorableSetting {
            key: setting.name().to_owned(),
        })?;
    repo.set_string(setting.name(), &encoded).await
}
