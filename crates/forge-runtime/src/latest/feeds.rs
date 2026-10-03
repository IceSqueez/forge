use std::collections::BTreeMap;

use forge_events::{DonationReceived, Event, NowPlaying, PlaybackState};
use forge_types::{
    CurrencyCode, LatestValue, MICROS_PER_MAJOR_UNIT, MoneyAmount, PlatformId, Variant,
    latest_fields,
};
use time::OffsetDateTime;

use crate::latest::slots::{FeedContext, SlotReading};

pub const SUPER_CHAT_KIND: &str = "youtube.chat.super_chat";
pub const SUPER_STICKER_KIND: &str = "youtube.chat.super_sticker";
pub const CHEER_KIND: &str = "twitch.channel.cheer";

pub const BITS_UNIT: &str = "BITS";

const DONATION_KIND_DONATION: &str = "donation";
const DONATION_KIND_SUPER_CHAT: &str = "super_chat";
const DONATION_KIND_SUPER_STICKER: &str = "super_sticker";
const DONATION_KIND_CHEER: &str = "cheer";

const YOUTUBE_AUTHOR: &str = "author";
const YOUTUBE_AMOUNT_MICROS: &str = "amount_micros";
const YOUTUBE_CURRENCY: &str = "currency";
const TWITCH_BITS: &str = "bits";
const TWITCH_IS_ANONYMOUS: &str = "is_anonymous";
const TWITCH_USER: &str = "user";
const TWITCH_LOGIN: &str = "login";
const DISPLAY_NAME: &str = "display_name";

struct ShownAmount {
    micros: u64,
    major_units: f64,
    formatted: String,
    unit: String,
}

impl ShownAmount {
    fn money(amount: &MoneyAmount) -> Self {
        Self {
            micros: amount.micros(),
            major_units: amount.major_units(),
            formatted: amount.formatted(),
            unit: amount.currency().as_str().to_owned(),
        }
    }

    fn bits(bits: u64) -> Option<Self> {
        Some(Self {
            micros: bits.checked_mul(MICROS_PER_MAJOR_UNIT)?,
            major_units: bits as f64,
            formatted: format!("{bits} {BITS_UNIT}"),
            unit: BITS_UNIT.to_owned(),
        })
    }
}

fn donation_value(
    platform: &str,
    occurred_at: OffsetDateTime,
    donor_name: String,
    donation_kind: &str,
    amount: ShownAmount,
) -> SlotReading {
    let fields = BTreeMap::from([
        (
            latest_fields::USER_NAME.to_owned(),
            Variant::String(donor_name),
        ),
        (
            latest_fields::AMOUNT.to_owned(),
            Variant::Float(amount.major_units),
        ),
        (
            latest_fields::AMOUNT_MICROS.to_owned(),
            Variant::Int(i64::try_from(amount.micros).unwrap_or(i64::MAX)),
        ),
        (
            latest_fields::AMOUNT_FORMATTED.to_owned(),
            Variant::String(amount.formatted),
        ),
        (
            latest_fields::CURRENCY.to_owned(),
            Variant::String(amount.unit),
        ),
        (
            latest_fields::DONATION_KIND.to_owned(),
            Variant::String(donation_kind.to_owned()),
        ),
    ]);
    SlotReading::Value(LatestValue::new(platform, occurred_at, fields))
}

fn shown_name(name: Option<&str>, context: &FeedContext<'_>) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(context.anonymous_name)
        .to_owned()
}

pub fn read_donation(event: &Event, _context: &FeedContext<'_>) -> Option<SlotReading> {
    let donation = DonationReceived::from_event(event)?;
    if donation.test || donation.amount_micros == 0 {
        return None;
    }
    let amount = ShownAmount::money(&donation.amount());
    Some(donation_value(
        donation.provider.as_str(),
        donation.occurred_at,
        donation.donor_name,
        DONATION_KIND_DONATION,
        amount,
    ))
}

fn read_youtube_support(
    event: &Event,
    context: &FeedContext<'_>,
    donation_kind: &str,
) -> Option<SlotReading> {
    let payload = &event.payload;
    let micros = payload.get(YOUTUBE_AMOUNT_MICROS)?.as_u64()?;
    if micros == 0 {
        return None;
    }
    let currency = CurrencyCode::parse(payload.get(YOUTUBE_CURRENCY)?.as_str()?).ok()?;
    let amount = MoneyAmount::from_micros(micros, currency);
    let author = payload
        .get(YOUTUBE_AUTHOR)
        .and_then(|author| author.get(DISPLAY_NAME))
        .and_then(|name| name.as_str());
    Some(donation_value(
        PlatformId::YouTube.as_str(),
        event.timestamp,
        shown_name(author, context),
        donation_kind,
        ShownAmount::money(&amount),
    ))
}

pub fn read_super_chat(event: &Event, context: &FeedContext<'_>) -> Option<SlotReading> {
    read_youtube_support(event, context, DONATION_KIND_SUPER_CHAT)
}

pub fn read_super_sticker(event: &Event, context: &FeedContext<'_>) -> Option<SlotReading> {
    read_youtube_support(event, context, DONATION_KIND_SUPER_STICKER)
}

pub fn read_cheer(event: &Event, context: &FeedContext<'_>) -> Option<SlotReading> {
    let payload = &event.payload;
    let bits = payload
        .get(TWITCH_BITS)?
        .as_u64()
        .filter(|bits| *bits > 0)?;
    let anonymous = payload
        .get(TWITCH_IS_ANONYMOUS)
        .and_then(|flag| flag.as_bool())
        .unwrap_or(false);
    let user = payload.get(TWITCH_USER);
    let display_name = user
        .and_then(|user| user.get(DISPLAY_NAME))
        .and_then(|name| name.as_str())
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            user.and_then(|user| user.get(TWITCH_LOGIN))
                .and_then(|login| login.as_str())
        });
    let name = if anonymous { None } else { display_name };
    Some(donation_value(
        PlatformId::Twitch.as_str(),
        event.timestamp,
        shown_name(name, context),
        DONATION_KIND_CHEER,
        ShownAmount::bits(bits)?,
    ))
}

pub fn read_now_playing(event: &Event, _context: &FeedContext<'_>) -> Option<SlotReading> {
    let now_playing = NowPlaying::from_event(event)?;
    let platform = now_playing.producer.as_str().to_owned();
    if now_playing.state == PlaybackState::Stopped {
        return Some(SlotReading::Ended { platform });
    }
    let fields = BTreeMap::from([
        (
            latest_fields::TITLE.to_owned(),
            Variant::String(now_playing.title),
        ),
        (
            latest_fields::ARTIST.to_owned(),
            Variant::String(now_playing.artist),
        ),
        (
            latest_fields::ALBUM.to_owned(),
            Variant::String(now_playing.album.unwrap_or_default()),
        ),
        (
            latest_fields::ART.to_owned(),
            Variant::String(now_playing.art.unwrap_or_default()),
        ),
    ]);
    Some(SlotReading::Value(LatestValue::new(
        platform,
        now_playing.occurred_at,
        fields,
    )))
}
