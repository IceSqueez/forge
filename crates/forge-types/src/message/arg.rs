use std::time::Duration;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

use crate::integration::IntegrationId;
use crate::message::verbatim::VerbatimText;
use crate::platform::PlatformId;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FiniteFloat(f64);

impl FiniteFloat {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for FiniteFloat {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| D::Error::custom("message argument float is not finite"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageArg {
    Int(i64),
    Float(FiniteFloat),
    #[serde(with = "time::serde::rfc3339")]
    Datetime(OffsetDateTime),
    Duration(Duration),
    Verbatim(VerbatimText),
    Integration(IntegrationId),
    Platform(PlatformId),
}

impl From<i64> for MessageArg {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<i32> for MessageArg {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<u32> for MessageArg {
    fn from(value: u32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<u64> for MessageArg {
    fn from(value: u64) -> Self {
        Self::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<usize> for MessageArg {
    fn from(value: usize) -> Self {
        Self::Int(i64::try_from(value).unwrap_or(i64::MAX))
    }
}

impl From<f64> for MessageArg {
    fn from(value: f64) -> Self {
        match FiniteFloat::new(value) {
            Some(finite) => Self::Float(finite),
            None => Self::Verbatim(VerbatimText::new(value.to_string())),
        }
    }
}

impl From<FiniteFloat> for MessageArg {
    fn from(value: FiniteFloat) -> Self {
        Self::Float(value)
    }
}

impl From<OffsetDateTime> for MessageArg {
    fn from(value: OffsetDateTime) -> Self {
        Self::Datetime(value)
    }
}

impl From<Duration> for MessageArg {
    fn from(value: Duration) -> Self {
        Self::Duration(value)
    }
}

impl From<VerbatimText> for MessageArg {
    fn from(value: VerbatimText) -> Self {
        Self::Verbatim(value)
    }
}

impl From<IntegrationId> for MessageArg {
    fn from(value: IntegrationId) -> Self {
        Self::Integration(value)
    }
}

impl From<PlatformId> for MessageArg {
    fn from(value: PlatformId) -> Self {
        Self::Platform(value)
    }
}
