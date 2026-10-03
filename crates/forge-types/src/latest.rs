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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn value_at(occurred_at: OffsetDateTime) -> LatestValue {
        LatestValue::new(
            "donatello",
            occurred_at,
            BTreeMap::from([(
                latest_fields::USER_NAME.to_owned(),
                Variant::String("SecretDonorName".to_owned()),
            )]),
        )
    }

    #[test]
    fn occurred_at_is_truncated_to_milliseconds_in_the_record_and_its_fields() {
        let precise = OffsetDateTime::from_unix_timestamp_nanos(1_790_000_000_123_456_789).unwrap();
        let truncated =
            OffsetDateTime::from_unix_timestamp_nanos(1_790_000_000_123_000_000).unwrap();

        let value = value_at(precise);

        assert_eq!(value.occurred_at, truncated);
        assert_eq!(
            value.fields.get(latest_fields::OCCURRED_AT),
            Some(&Variant::Datetime(truncated))
        );
    }

    #[test]
    fn platform_and_occurred_at_fields_override_caller_supplied_ones() {
        let at = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();
        let fields = BTreeMap::from([(
            latest_fields::PLATFORM.to_owned(),
            Variant::String("spoofed".to_owned()),
        )]);

        let value = LatestValue::new("donatello", at, fields);

        assert_eq!(
            value.fields.get(latest_fields::PLATFORM),
            Some(&Variant::String("donatello".to_owned()))
        );
    }

    #[test]
    fn blank_platform_filter_means_most_recent_across_platforms() {
        for (filter, expected) in [
            ("", LatestScope::MostRecentAcrossPlatforms),
            ("   ", LatestScope::MostRecentAcrossPlatforms),
            ("twitch", LatestScope::Platform("twitch")),
            (" twitch ", LatestScope::Platform("twitch")),
        ] {
            assert_eq!(
                LatestScope::from_platform_filter(filter),
                expected,
                "{filter:?}"
            );
        }
    }
}
