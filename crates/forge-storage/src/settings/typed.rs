use std::marker::PhantomData;

use forge_types::LogLevel;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::decode_bool_setting;
use super::disclosure::SettingDisclosure;
use super::portability::SettingPortability;
use super::row::{RowClasses, RowGate};
use super::spec::{SettingEncoding, SettingSpec};
use crate::{SettingsRepo, StorageError};

pub trait SettingCodec: Send + Sync {
    type Value: Send + Sync;
    const ENCODING: SettingEncoding;

    fn default_value(&self) -> Self::Value;
    fn decode(&self, raw: &str) -> Option<Self::Value>;
    fn encode(&self, value: &Self::Value) -> Option<String>;

    fn is_absent(&self, _value: &Self::Value) -> bool {
        false
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub default: u64,
}

impl SettingCodec for Tally {
    type Value = u64;
    const ENCODING: SettingEncoding = SettingEncoding::Integer;

    fn default_value(&self) -> u64 {
        self.default
    }

    fn decode(&self, raw: &str) -> Option<u64> {
        raw.trim().parse().ok()
    }

    fn encode(&self, value: &u64) -> Option<String> {
        Some(value.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlainText {
    pub default: &'static str,
}

impl SettingCodec for PlainText {
    type Value = String;
    const ENCODING: SettingEncoding = SettingEncoding::Text;

    fn default_value(&self) -> String {
        self.default.to_owned()
    }

    fn decode(&self, raw: &str) -> Option<String> {
        Some(raw.to_owned())
    }

    fn encode(&self, value: &String) -> Option<String> {
        Some(value.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionalText;

impl SettingCodec for OptionalText {
    type Value = Option<String>;
    const ENCODING: SettingEncoding = SettingEncoding::Text;

    fn default_value(&self) -> Option<String> {
        None
    }

    fn decode(&self, raw: &str) -> Option<Option<String>> {
        Some(Some(raw.to_owned()))
    }

    fn encode(&self, value: &Option<String>) -> Option<String> {
        value.clone()
    }

    fn is_absent(&self, value: &Option<String>) -> bool {
        value.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub default: &'static str,
    pub options: &'static [&'static str],
}

impl SettingCodec for Choice {
    type Value = String;
    const ENCODING: SettingEncoding = SettingEncoding::Text;

    fn default_value(&self) -> String {
        self.default.to_owned()
    }

    fn decode(&self, raw: &str) -> Option<String> {
        self.options
            .iter()
            .find(|option| **option == raw)
            .map(|option| (*option).to_owned())
    }

    fn encode(&self, value: &String) -> Option<String> {
        self.options
            .contains(&value.as_str())
            .then(|| value.clone())
    }
}

pub struct JsonDocument<T>(PhantomData<fn() -> T>);

impl<T> JsonDocument<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for JsonDocument<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for JsonDocument<T> {}

impl<T> std::fmt::Debug for JsonDocument<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JsonDocument")
    }
}

impl<T> PartialEq for JsonDocument<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Default for JsonDocument<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SettingCodec for JsonDocument<T>
where
    T: Default + Serialize + DeserializeOwned + Send + Sync,
{
    type Value = T;
    const ENCODING: SettingEncoding = SettingEncoding::Json;

    fn default_value(&self) -> T {
        T::default()
    }

    fn decode(&self, raw: &str) -> Option<T> {
        serde_json::from_str(raw).ok()
    }

    fn encode(&self, value: &T) -> Option<String> {
        serde_json::to_string(value).ok()
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

pub async fn read_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    setting: &Setting<C>,
) -> Result<C::Value, StorageError> {
    let raw = repo.get_string(setting.name()).await?;
    Ok(setting.decode_or_default(raw.as_deref()))
}

pub async fn read_stored_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    setting: &Setting<C>,
) -> Result<Option<C::Value>, StorageError> {
    repo.get_string(setting.name())
        .await?
        .map(|raw| {
            setting
                .codec
                .decode(&raw)
                .ok_or_else(|| StorageError::UndecodableSetting {
                    key: setting.name().to_owned(),
                })
        })
        .transpose()
}

pub async fn read_setting_or_default<C: SettingCodec>(
    repo: &dyn SettingsRepo,
    setting: &Setting<C>,
) -> C::Value {
    match read_setting(repo, setting).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                key = setting.name(),
                error = %error,
                "stored setting cannot be read; using its default"
            );
            setting.codec.default_value()
        }
    }
}

pub(crate) async fn put_declared<R: SettingsRepo + ?Sized>(
    repo: &R,
    spec: &SettingSpec,
    key: &str,
    value: &str,
) -> Result<(), StorageError> {
    repo.put_row(RowGate::open(), key, value, RowClasses::from(spec))
        .await
}

pub async fn write_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    setting: &Setting<C>,
    value: &C::Value,
) -> Result<(), StorageError> {
    if setting.codec.is_absent(value) {
        reset_setting(repo, setting).await?;
        return Ok(());
    }
    let encoded = setting
        .codec
        .encode(value)
        .ok_or_else(|| StorageError::UnstorableSetting {
            key: setting.name().to_owned(),
        })?;
    put_declared(repo, &setting.spec, setting.name(), &encoded).await
}

pub async fn reset_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    setting: &Setting<C>,
) -> Result<bool, StorageError> {
    repo.delete(setting.name()).await
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingFamily<C> {
    spec: SettingSpec,
    codec: C,
}

impl<C: SettingCodec> SettingFamily<C> {
    pub const fn new(
        prefix: &'static str,
        codec: C,
        portability: SettingPortability,
        disclosure: SettingDisclosure,
    ) -> Self {
        Self {
            spec: SettingSpec::prefix(prefix, C::ENCODING, portability, disclosure),
            codec,
        }
    }

    pub const fn spec(&self) -> SettingSpec {
        self.spec
    }

    pub const fn codec(&self) -> &C {
        &self.codec
    }

    pub const fn prefix(&self) -> &'static str {
        self.spec.key.name()
    }

    pub fn member_key(&self, member: &str) -> Result<String, StorageError> {
        if !is_valid_member(member) {
            return Err(StorageError::InvalidSettingMember {
                family: self.prefix().to_owned(),
                member: member.to_owned(),
            });
        }
        Ok(format!("{}{member}", self.prefix()))
    }

    fn decode_or_default(&self, key: &str, raw: Option<&str>) -> C::Value {
        let Some(raw) = raw else {
            return self.codec.default_value();
        };
        self.codec.decode(raw).unwrap_or_else(|| {
            tracing::warn!(key, "stored setting cannot be decoded; using its default");
            self.codec.default_value()
        })
    }
}

pub(crate) fn is_valid_member(member: &str) -> bool {
    !member.is_empty() && !member.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub async fn read_family_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    family: &SettingFamily<C>,
    member: &str,
) -> Result<C::Value, StorageError> {
    let key = family.member_key(member)?;
    let raw = repo.get_string(&key).await?;
    Ok(family.decode_or_default(&key, raw.as_deref()))
}

pub async fn write_family_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    family: &SettingFamily<C>,
    member: &str,
    value: &C::Value,
) -> Result<(), StorageError> {
    let key = family.member_key(member)?;
    let encoded = family
        .codec
        .encode(value)
        .ok_or_else(|| StorageError::UnstorableSetting { key: key.clone() })?;
    put_declared(repo, &family.spec, &key, &encoded).await
}

pub async fn reset_family_setting<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    family: &SettingFamily<C>,
    member: &str,
) -> Result<bool, StorageError> {
    let key = family.member_key(member)?;
    repo.delete(&key).await
}

pub async fn family_members<C: SettingCodec, R: SettingsRepo + ?Sized>(
    repo: &R,
    family: &SettingFamily<C>,
) -> Result<Vec<(String, C::Value)>, StorageError> {
    let mut members: Vec<(String, C::Value)> = repo
        .load_all()
        .await?
        .into_iter()
        .filter_map(|(key, raw)| {
            let member = key.strip_prefix(family.prefix())?;
            if member.is_empty() {
                return None;
            }
            let value = family.decode_or_default(&key, Some(&raw));
            Some((member.to_owned(), value))
        })
        .collect();
    members.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(members)
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Write as _};
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::*;
    use crate::test_support::MemorySettings;

    const BOUNDED: Count = Count {
        default: 50,
        min: 1,
        max: 100,
        unclamped: None,
    };
    const WITH_UNLIMITED: Count = Count {
        unclamped: Some(0),
        ..BOUNDED
    };
    const UNIT: Level = Level {
        default: 0.5,
        min: 0.0,
        max: 1.0,
    };
    const VOLUME: Setting<Level> = Setting::new(
        "test.volume",
        UNIT,
        SettingPortability::Portable,
        SettingDisclosure::Verbatim,
    );
    const LIMIT: Setting<Count> = Setting::new(
        "test.limit",
        BOUNDED,
        SettingPortability::Portable,
        SettingDisclosure::Verbatim,
    );
    const SECRET_RAW: &str = "hunter2-not-a-number";

    #[derive(Clone, Default)]
    struct CapturedEvents(Arc<Mutex<Vec<String>>>);

    struct FieldText(String);

    impl Visit for FieldText {
        fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
            write!(self.0, "{}={value:?} ", field.name()).unwrap();
        }
    }

    impl Subscriber for CapturedEvents {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            let mut text = FieldText(String::new());
            event.record(&mut text);
            self.0.lock().unwrap().push(text.0);
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    fn events_while(run: impl FnOnce()) -> Vec<String> {
        let captured = CapturedEvents::default();
        tracing::subscriber::with_default(captured.clone(), run);
        captured.0.lock().unwrap().clone()
    }

    #[test]
    fn count_decode_clamps_into_range_at_and_around_each_bound() {
        for (raw, expected) in [
            ("0", 1),
            ("1", 1),
            ("2", 2),
            ("99", 99),
            ("100", 100),
            ("101", 100),
            ("4294967295", 100),
            (" 7 ", 7),
        ] {
            assert_eq!(BOUNDED.decode(raw), Some(expected), "raw {raw:?}");
        }
    }

    #[test]
    fn count_decode_rejects_text_that_is_not_an_unsigned_integer() {
        for raw in ["", "   ", "many", "-1", "1.5", "4294967296", "1e3"] {
            assert_eq!(BOUNDED.decode(raw), None, "raw {raw:?}");
        }
    }

    #[test]
    fn count_unclamped_sentinel_is_kept_while_its_neighbours_clamp() {
        for (value, stored) in [(0, "0"), (1, "1"), (101, "100")] {
            assert_eq!(
                WITH_UNLIMITED.encode(&value).as_deref(),
                Some(stored),
                "value {value}",
            );
        }
        assert_eq!(WITH_UNLIMITED.decode("0"), Some(0));
    }

    #[test]
    fn level_decode_refuses_text_that_is_not_a_finite_number() {
        for raw in ["", "loud", "NaN", "nan", "inf", "-inf", "infinity", "1e39"] {
            assert_eq!(UNIT.decode(raw), None, "raw {raw:?}");
        }
    }

    #[test]
    fn level_decode_clamps_finite_values_at_and_beyond_each_bound() {
        for (raw, expected) in [
            ("-0.5", 0.0),
            ("0", 0.0),
            ("0.25", 0.25),
            ("1", 1.0),
            ("1.5", 1.0),
        ] {
            assert_eq!(UNIT.decode(raw), Some(expected), "raw {raw:?}");
        }
    }

    #[test]
    fn level_encode_refuses_non_finite_values() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(UNIT.encode(&value), None, "value {value}");
        }
    }

    #[test]
    fn level_encode_clamps_finite_values_into_range() {
        for (value, stored) in [(-0.5, "0"), (0.25, "0.25"), (1.0, "1"), (1.5, "1")] {
            assert_eq!(
                UNIT.encode(&value).as_deref(),
                Some(stored),
                "value {value}"
            );
        }
    }

    #[test]
    fn decode_or_default_falls_back_only_for_absent_or_undecodable_text() {
        for (raw, expected) in [
            (None, 0.5),
            (Some("loud"), 0.5),
            (Some("NaN"), 0.5),
            (Some("0.25"), 0.25),
        ] {
            assert_eq!(VOLUME.decode_or_default(raw), expected, "raw {raw:?}");
        }
    }

    #[test]
    fn an_undecodable_stored_value_is_warned_by_key_without_its_text() {
        let events = events_while(|| {
            VOLUME.decode_or_default(Some(SECRET_RAW));
        });

        assert_eq!(
            (
                events.len(),
                events.iter().any(|e| e.contains(VOLUME.name())),
                events.iter().any(|e| e.contains(SECRET_RAW)),
            ),
            (1, true, false),
            "{events:?}",
        );
    }

    #[test]
    fn an_absent_or_decodable_value_reads_without_a_warning() {
        let events = events_while(|| {
            VOLUME.decode_or_default(None);
            VOLUME.decode_or_default(Some("0.25"));
        });

        assert_eq!(events, Vec::<String>::new());
    }

    #[tokio::test]
    async fn read_setting_decodes_the_stored_text_through_the_codec() {
        let repo = MemorySettings::new();
        repo.seed_raw(&LIMIT, "250");

        assert_eq!(read_setting(&repo, &LIMIT).await.unwrap(), 100);
    }

    fn unreachable_repo() -> MemorySettings {
        let repo = MemorySettings::new();
        repo.fail_reads(true);
        repo
    }

    #[tokio::test]
    async fn read_setting_or_default_serves_the_default_when_the_store_cannot_be_read() {
        assert_eq!(
            read_setting_or_default(&unreachable_repo(), &LIMIT).await,
            BOUNDED.default
        );
    }

    #[tokio::test]
    async fn read_stored_setting_reports_an_absent_row_as_none_not_the_default() {
        let repo = MemorySettings::new();

        assert_eq!(read_stored_setting(&repo, &LIMIT).await.unwrap(), None);
    }

    #[tokio::test]
    async fn read_stored_setting_decodes_a_present_row_through_the_codec() {
        let repo = MemorySettings::new();
        repo.seed_raw(&LIMIT, "250");

        assert_eq!(
            read_stored_setting(&repo, &LIMIT).await.unwrap(),
            Some(BOUNDED.max)
        );
    }

    #[tokio::test]
    async fn read_stored_setting_fails_on_an_undecodable_row_naming_the_key_not_its_text() {
        let repo = MemorySettings::new();
        repo.seed_raw(&LIMIT, SECRET_RAW);

        let result = read_stored_setting(&repo, &LIMIT).await;

        assert!(
            matches!(&result, Err(StorageError::UndecodableSetting { key }) if key == LIMIT.name()),
            "{result:?}"
        );
        assert!(!result.unwrap_err().to_string().contains(SECRET_RAW));
    }

    #[tokio::test]
    async fn read_stored_setting_passes_a_connection_error_through_unchanged() {
        let result = read_stored_setting(&unreachable_repo(), &LIMIT).await;

        assert!(
            matches!(&result, Err(StorageError::Connection { reason }) if reason == crate::test_support::INJECTED_FAILURE_REASON),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn write_setting_refuses_a_non_finite_level_naming_the_key_and_stores_nothing() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let repo = MemorySettings::new();

            let result = write_setting(&repo, &VOLUME, &value).await;

            assert!(
                matches!(&result, Err(StorageError::UnstorableSetting { key }) if key == VOLUME.name()),
                "value {value}: {result:?}",
            );
            assert_eq!(repo.rows().len(), 0, "value {value}");
        }
    }
}
