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

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Write as _};
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Metadata, Subscriber};

    use super::*;
    use crate::event_log::tests::MapRepo;

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
        let repo = MapRepo::default();
        repo.set_string(LIMIT.name(), "250").await.unwrap();

        assert_eq!(read_setting(&repo, &LIMIT).await.unwrap(), 100);
    }

    #[tokio::test]
    async fn write_setting_refuses_a_non_finite_level_naming_the_key_and_stores_nothing() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let repo = MapRepo::default();

            let result = write_setting(&repo, &VOLUME, &value).await;

            assert!(
                matches!(&result, Err(StorageError::UnstorableSetting { key }) if key == VOLUME.name()),
                "value {value}: {result:?}",
            );
            assert_eq!(repo.load_all().await.unwrap().len(), 0, "value {value}");
        }
    }
}
