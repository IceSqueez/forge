use std::collections::BTreeMap;

use forge_types::{
    ArgStack, LATEST_DONATION_SLOT, LatestScope, LatestValue, NOW_PLAYING_SLOT, Variant,
    latest_fields,
};
use time::OffsetDateTime;

use crate::config::{
    HEADLINE, LABEL, PLACEHOLDER, PLATFORM, SLOT, SUBLINE, effective_overlay_config, read_str,
};
use crate::descriptor::{ContentFeed, OverlayConfig, OverlayKindDescriptor};

pub const SLOT_OPTIONS: &[&str] = &[LATEST_DONATION_SLOT, NOW_PLAYING_SLOT];

const SAMPLE_OCCURRED_AT_UNIX_SECS: i64 = 1_790_000_000;
const SAMPLE_DONATION_PLATFORM: &str = "donatello";
const SAMPLE_PLAYER_PLATFORM: &str = "player";
const SAMPLE_AMOUNT: f64 = 100.0;
const SAMPLE_AMOUNT_MICROS: i64 = 100_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestBinding {
    pub slot: String,
    pub platform: String,
}

impl LatestBinding {
    pub fn scope(&self) -> LatestScope<'_> {
        LatestScope::from_platform_filter(&self.platform)
    }
}

pub fn latest_binding(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> Option<LatestBinding> {
    if descriptor.content_feed() != ContentFeed::LatestSlot {
        return None;
    }
    let config = effective_overlay_config(descriptor, stored);
    Some(LatestBinding {
        slot: read_str(&config, SLOT).to_owned(),
        platform: read_str(&config, PLATFORM).to_owned(),
    })
}

pub fn latest_content(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
    value: Option<&LatestValue>,
) -> OverlayConfig {
    let config = effective_overlay_config(descriptor, stored);
    let args = value.map(record_args).unwrap_or_default();
    let expand = |key: &str| Variant::String(args.interpolate(read_str(&config, key)));

    let mut content = OverlayConfig::from([(LABEL.to_owned(), expand(LABEL))]);
    match value {
        Some(_) => {
            content.insert(HEADLINE.to_owned(), expand(HEADLINE));
            content.insert(SUBLINE.to_owned(), expand(SUBLINE));
        }
        None => {
            content.insert(PLACEHOLDER.to_owned(), expand(PLACEHOLDER));
        }
    }
    content
}

pub fn sample_latest_value(slot: &str) -> LatestValue {
    let occurred_at = OffsetDateTime::from_unix_timestamp(SAMPLE_OCCURRED_AT_UNIX_SECS)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    if slot == NOW_PLAYING_SLOT {
        return LatestValue::new(
            SAMPLE_PLAYER_PLATFORM,
            occurred_at,
            texts(&[
                (latest_fields::TITLE, "Midnight Drive"),
                (latest_fields::ARTIST, "Nova Lines"),
                (latest_fields::ALBUM, "Night Shift"),
            ]),
        );
    }
    let mut fields = texts(&[
        (latest_fields::USER_NAME, "PixelPal"),
        (latest_fields::AMOUNT_FORMATTED, "100.00 UAH"),
        (latest_fields::CURRENCY, "UAH"),
        (latest_fields::DONATION_KIND, "donation"),
    ]);
    fields.insert(
        latest_fields::AMOUNT.to_owned(),
        Variant::Float(SAMPLE_AMOUNT),
    );
    fields.insert(
        latest_fields::AMOUNT_MICROS.to_owned(),
        Variant::Int(SAMPLE_AMOUNT_MICROS),
    );
    LatestValue::new(SAMPLE_DONATION_PLATFORM, occurred_at, fields)
}

fn texts(pairs: &[(&str, &str)]) -> BTreeMap<String, Variant> {
    pairs
        .iter()
        .map(|(key, text)| ((*key).to_owned(), Variant::String((*text).to_owned())))
        .collect()
}

fn record_args(value: &LatestValue) -> ArgStack {
    value
        .fields
        .iter()
        .fold(ArgStack::new(), |stack, (name, field)| {
            stack.set(name.clone(), field.clone())
        })
}
