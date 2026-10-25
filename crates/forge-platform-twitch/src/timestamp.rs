use forge_types::{ArgStack, Variant, datetime_from_text};
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::well_known::Iso8601;

pub(crate) fn parse(text: &str) -> Option<OffsetDateTime> {
    datetime_from_text(text).or_else(|| {
        PrimitiveDateTime::parse(text.trim(), &Iso8601::DEFAULT)
            .ok()
            .map(PrimitiveDateTime::assume_utc)
    })
}

pub(crate) fn variant(text: &str) -> Option<Variant> {
    let parsed = parse(text);
    if parsed.is_none() {
        tracing::debug!(
            len = text.len(),
            "twitch timestamp not parseable, variable omitted"
        );
    }
    parsed.map(Variant::Datetime)
}

pub(crate) fn variant_of(value: Option<&serde_json::Value>) -> Option<Variant> {
    value.and_then(|raw| raw.as_str()).and_then(variant)
}

pub(crate) fn set(stack: ArgStack, name: &str, value: Option<Variant>) -> ArgStack {
    match value {
        Some(value) => stack.set(name.to_owned(), value),
        None => stack,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use time::format_description::well_known::Rfc3339;
    use time::{Date, Month, Time, UtcOffset};

    pub(crate) fn instant(rfc3339: &str) -> Variant {
        Variant::Datetime(OffsetDateTime::parse(rfc3339, &Rfc3339).unwrap())
    }

    fn june_13(hour: u8, nanos: u32, offset_hours: i8) -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::June, 13).unwrap(),
            Time::from_hms_nano(hour, 0, 0, nanos).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(offset_hours, 0, 0).unwrap())
    }

    #[test]
    fn a_twitch_timestamp_becomes_the_instant_it_names() {
        for (text, expected) in [
            ("2026-06-13T18:00:00Z", june_13(18, 0, 0)),
            ("2026-06-13T21:00:00+03:00", june_13(18, 0, 0)),
            (
                "2026-06-13T18:00:00.594466284Z",
                june_13(18, 594_466_284, 0),
            ),
            ("2026-06-13T18:00:00", june_13(18, 0, 0)),
            ("2026-06-13T18:00:00.5", june_13(18, 500_000_000, 0)),
            (" 2026-06-13T18:00:00Z ", june_13(18, 0, 0)),
        ] {
            assert_eq!(variant(text), Some(Variant::Datetime(expected)), "{text:?}");
        }
    }

    #[test]
    fn a_missing_or_unreadable_timestamp_yields_no_value() {
        for (raw, what) in [
            (None, "absent key"),
            (Some(serde_json::Value::Null), "null"),
            (Some(serde_json::json!("")), "empty string"),
            (Some(serde_json::json!("   ")), "whitespace"),
            (Some(serde_json::json!("soon")), "garbage"),
            (Some(serde_json::json!("2026-06-13")), "date without time"),
            (
                Some(serde_json::json!("2026-13-40T00:00:00Z")),
                "impossible date",
            ),
            (
                Some(serde_json::json!(1_781_373_600)),
                "unix seconds number",
            ),
        ] {
            assert_eq!(variant_of(raw.as_ref()), None, "{what}");
        }
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn an_unreadable_timestamp_is_logged_by_length_without_its_text() {
        let captured = Captured::default();
        let writer = captured.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();

        tracing::subscriber::with_default(subscriber, || variant("viewer-typed-payload"));

        let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("len=20"), "{log}");
        assert!(!log.contains("viewer-typed-payload"), "{log}");
    }
}
