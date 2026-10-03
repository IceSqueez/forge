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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use forge_events::EventSource;
    use forge_types::{DonorVisibility, IntegrationId};
    use serde_json::{Value, json};

    use super::*;

    type Read = fn(&Event, &FeedContext<'_>) -> Option<SlotReading>;

    const ANONYMOUS: &str = "Anonymous donor";
    const PRIVATE_MESSAGE: &str = "my private message";

    fn context() -> FeedContext<'static> {
        FeedContext {
            anonymous_name: ANONYMOUS,
        }
    }

    fn occurred_at() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap()
    }

    fn event_at(source: EventSource, kind: &str, payload: Value) -> Event {
        let mut event = Event::new(source, kind, payload);
        event.timestamp = occurred_at() + time::Duration::seconds(5);
        event
    }

    fn donation_event(amount_micros: u64) -> Event {
        DonationReceived {
            provider: IntegrationId::new("donatello"),
            donation_id: "dl-1".to_owned(),
            donor_name: "Olena".to_owned(),
            donor_visibility: DonorVisibility::Named,
            message: Some(PRIVATE_MESSAGE.to_owned()),
            amount_micros,
            currency: CurrencyCode::parse("UAH").unwrap(),
            occurred_at: occurred_at(),
            test: false,
        }
        .into_event()
        .unwrap()
    }

    fn youtube_event(kind: &str, micros: u64, currency: &str) -> Event {
        event_at(
            EventSource::YouTube,
            kind,
            json!({
                "amount_micros": micros,
                "currency": currency,
                "message": PRIVATE_MESSAGE,
                "author": { "channel_id": "UC1", "display_name": "Mykola" },
            }),
        )
    }

    fn cheer_event(bits: u64, anonymous: bool, display_name: &str, login: &str) -> Event {
        event_at(
            EventSource::Twitch,
            CHEER_KIND,
            json!({
                "bits": bits,
                "is_anonymous": anonymous,
                "message": PRIVATE_MESSAGE,
                "user": { "id": "1", "login": login, "display_name": display_name },
            }),
        )
    }

    fn value_of(reading: Option<SlotReading>) -> LatestValue {
        match reading {
            Some(SlotReading::Value(value)) => value,
            other => panic!("expected a recorded value, got {other:?}"),
        }
    }

    fn text(value: &LatestValue, field: &str) -> String {
        match value.fields.get(field) {
            Some(Variant::String(text)) => text.clone(),
            other => panic!("{field} is not a string: {other:?}"),
        }
    }

    #[test]
    fn each_feeding_kind_records_platform_name_amount_and_kind() {
        let later = occurred_at() + time::Duration::seconds(5);
        let cases: [(Event, Read, _); 4] = [
            (
                donation_event(12_500_000),
                read_donation,
                (
                    "donatello",
                    "Olena",
                    "12.50 UAH",
                    "UAH",
                    "donation",
                    12_500_000_i64,
                    occurred_at(),
                ),
            ),
            (
                youtube_event(SUPER_CHAT_KIND, 5_000_000, "USD"),
                read_super_chat,
                (
                    "youtube",
                    "Mykola",
                    "5 USD",
                    "USD",
                    "super_chat",
                    5_000_000,
                    later,
                ),
            ),
            (
                youtube_event(SUPER_STICKER_KIND, 2_000_000, "EUR"),
                read_super_sticker,
                (
                    "youtube",
                    "Mykola",
                    "2 EUR",
                    "EUR",
                    "super_sticker",
                    2_000_000,
                    later,
                ),
            ),
            (
                cheer_event(500, false, "GenerousOne", "generous_one"),
                read_cheer,
                (
                    "twitch",
                    "GenerousOne",
                    "500 BITS",
                    "BITS",
                    "cheer",
                    500_000_000,
                    later,
                ),
            ),
        ];
        for (event, read, (platform, name, formatted, unit, kind, micros, when)) in cases {
            let value = value_of(read(&event, &context()));

            assert_eq!(
                (
                    value.platform.as_str(),
                    text(&value, latest_fields::USER_NAME).as_str(),
                    text(&value, latest_fields::AMOUNT_FORMATTED).as_str(),
                    text(&value, latest_fields::CURRENCY).as_str(),
                    text(&value, latest_fields::DONATION_KIND).as_str(),
                    value.fields.get(latest_fields::AMOUNT_MICROS),
                    value.occurred_at,
                ),
                (
                    platform,
                    name,
                    formatted,
                    unit,
                    kind,
                    Some(&Variant::Int(micros)),
                    when
                ),
                "{}",
                event.kind
            );
        }
    }

    #[test]
    fn cheer_amount_is_the_bit_count_in_major_units() {
        let value = value_of(read_cheer(
            &cheer_event(500, false, "GenerousOne", "generous_one"),
            &context(),
        ));

        assert_eq!(
            value.fields.get(latest_fields::AMOUNT),
            Some(&Variant::Float(500.0))
        );
    }

    #[test]
    fn zero_amounts_are_not_recorded() {
        let cases: [(Event, Read); 4] = [
            (donation_event(0), read_donation),
            (youtube_event(SUPER_CHAT_KIND, 0, "USD"), read_super_chat),
            (
                youtube_event(SUPER_STICKER_KIND, 0, "USD"),
                read_super_sticker,
            ),
            (
                cheer_event(0, false, "GenerousOne", "generous_one"),
                read_cheer,
            ),
        ];
        for (event, read) in cases {
            assert_eq!(read(&event, &context()), None, "{}", event.kind);
        }
    }

    #[test]
    fn test_donation_is_never_read_as_a_value() {
        let mut event = donation_event(12_500_000);
        event.payload["test"] = json!(true);

        assert_eq!(read_donation(&event, &context()), None);
    }

    #[test]
    fn missing_or_unparsable_youtube_currency_is_not_recorded() {
        for currency in [
            json!(""),
            json!("U5D"),
            json!("US"),
            json!("DOLLARS"),
            Value::Null,
        ] {
            let mut event = youtube_event(SUPER_CHAT_KIND, 5_000_000, "USD");
            event.payload["currency"] = currency.clone();

            assert_eq!(read_super_chat(&event, &context()), None, "{currency}");
        }
    }

    #[test]
    fn cheer_donor_name_follows_anonymity_then_display_name_then_login() {
        for (anonymous, display_name, login, expected) in [
            (true, "GenerousOne", "generous_one", ANONYMOUS),
            (false, "", "generous_one", "generous_one"),
            (false, "   ", "generous_one", "generous_one"),
            (false, "", "", ANONYMOUS),
        ] {
            let value = value_of(read_cheer(
                &cheer_event(100, anonymous, display_name, login),
                &context(),
            ));

            assert_eq!(
                text(&value, latest_fields::USER_NAME),
                expected,
                "anonymous={anonymous} display={display_name:?} login={login:?}"
            );
        }
    }

    #[test]
    fn super_chat_without_an_author_name_shows_the_anonymous_placeholder() {
        let mut event = youtube_event(SUPER_CHAT_KIND, 5_000_000, "USD");
        event.payload["author"] = json!({ "channel_id": "UC1" });

        let value = value_of(read_super_chat(&event, &context()));

        assert_eq!(text(&value, latest_fields::USER_NAME), ANONYMOUS);
    }

    #[test]
    fn recorded_value_never_carries_the_donor_message() {
        let cases: [(Event, Read); 4] = [
            (donation_event(12_500_000), read_donation),
            (
                youtube_event(SUPER_CHAT_KIND, 5_000_000, "USD"),
                read_super_chat,
            ),
            (
                youtube_event(SUPER_STICKER_KIND, 5_000_000, "USD"),
                read_super_sticker,
            ),
            (
                cheer_event(500, false, "GenerousOne", "generous_one"),
                read_cheer,
            ),
        ];
        for (event, read) in cases {
            let stored = value_of(read(&event, &context())).to_variant().to_json();

            assert!(stored.get("message").is_none(), "{}", event.kind);
            assert!(
                !stored.to_string().contains(PRIVATE_MESSAGE),
                "{}",
                event.kind
            );
        }
    }
}
