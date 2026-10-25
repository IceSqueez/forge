use forge_types::{Variant, datetime_from_text};
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

pub(crate) fn parse_logged(text: &str) -> Option<OffsetDateTime> {
    let parsed = parse(text);
    if parsed.is_none() {
        tracing::debug!(
            len = text.len(),
            "kick timestamp not parseable, variable omitted"
        );
    }
    parsed
}

pub(crate) fn variant(value: Option<OffsetDateTime>) -> Option<Variant> {
    value.map(Variant::Datetime)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use time::format_description::well_known::Rfc3339;

    pub(crate) fn instant(rfc3339: &str) -> OffsetDateTime {
        OffsetDateTime::parse(rfc3339, &Rfc3339).unwrap()
    }

    #[test]
    fn a_kick_timestamp_becomes_the_instant_it_names() {
        for (text, expected) in [
            ("2026-06-13T18:00:00Z", "2026-06-13T18:00:00Z"),
            ("2026-06-13T21:00:00+03:00", "2026-06-13T18:00:00Z"),
            (
                "2026-06-13T18:00:00.594466284Z",
                "2026-06-13T18:00:00.594466284Z",
            ),
            ("2026-06-13T18:00:00", "2026-06-13T18:00:00Z"),
            ("2026-06-13T18:00:00.5", "2026-06-13T18:00:00.5Z"),
            (" 2026-06-13T18:00:00Z ", "2026-06-13T18:00:00Z"),
        ] {
            assert_eq!(parse(text), Some(instant(expected)), "{text:?}");
        }
    }

    #[test]
    fn an_empty_or_unreadable_timestamp_yields_no_instant() {
        for (text, what) in [
            ("", "empty string"),
            ("   ", "whitespace"),
            ("soon", "garbage"),
            ("2026-06-13", "date without time"),
            ("2026-13-40T00:00:00Z", "impossible date"),
            ("1781373600", "unix seconds as text"),
        ] {
            assert_eq!(parse(text), None, "{what}");
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

        let parsed =
            tracing::subscriber::with_default(subscriber, || parse_logged("viewer-typed-payload"));

        let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert_eq!(parsed, None);
        assert!(log.contains("len=20"), "{log}");
        assert!(!log.contains("viewer-typed-payload"), "{log}");
    }
}
