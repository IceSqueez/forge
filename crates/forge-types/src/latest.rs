use std::collections::BTreeMap;
use std::fmt;

use time::OffsetDateTime;

use crate::Variant;

pub const LATEST_DONATION_SLOT: &str = "donation";
pub const NOW_PLAYING_SLOT: &str = "now_playing";

pub mod latest_fields {
    pub const PLATFORM: &str = "platform";
    pub const OCCURRED_AT: &str = "occurred_at";
    pub const USER_NAME: &str = "user_name";
    pub const AMOUNT: &str = "amount";
    pub const AMOUNT_MICROS: &str = "amount_micros";
    pub const AMOUNT_FORMATTED: &str = "amount_formatted";
    pub const CURRENCY: &str = "currency";
    pub const DONATION_KIND: &str = "donation_kind";
    pub const TITLE: &str = "title";
    pub const ARTIST: &str = "artist";
    pub const ALBUM: &str = "album";
    pub const ART: &str = "art";
}

#[derive(Clone, PartialEq)]
pub struct LatestValue {
    pub platform: String,
    pub occurred_at: OffsetDateTime,
    pub fields: BTreeMap<String, Variant>,
}

impl LatestValue {
    pub fn new(
        platform: impl Into<String>,
        occurred_at: OffsetDateTime,
        mut fields: BTreeMap<String, Variant>,
    ) -> Self {
        let platform = platform.into();
        let occurred_at = occurred_at
            .replace_millisecond(occurred_at.millisecond())
            .unwrap_or(occurred_at);
        fields.insert(
            latest_fields::PLATFORM.to_owned(),
            Variant::String(platform.clone()),
        );
        fields.insert(
            latest_fields::OCCURRED_AT.to_owned(),
            Variant::Datetime(occurred_at),
        );
        Self {
            platform,
            occurred_at,
            fields,
        }
    }

    pub fn to_variant(&self) -> Variant {
        Variant::Object(self.fields.clone())
    }
}

impl fmt::Debug for LatestValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LatestValue")
            .field("platform", &self.platform)
            .field("occurred_at", &self.occurred_at)
            .field("fields", &self.fields.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatestScope<'a> {
    MostRecentAcrossPlatforms,
    Platform(&'a str),
}

impl<'a> LatestScope<'a> {
    pub fn from_platform_filter(platform: &'a str) -> Self {
        match platform.trim() {
            "" => Self::MostRecentAcrossPlatforms,
            platform => Self::Platform(platform),
        }
    }
}

pub trait LatestValueReader: Send + Sync {
    fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue>;
}
