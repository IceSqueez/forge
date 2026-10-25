use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn datetime_to_text(value: &OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

pub fn datetime_from_text(text: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(text.trim(), &Rfc3339).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_disclosure::DisclosedValue;
    use crate::variant::Variant;
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    fn at(year: i32, nanos: u32, offset: (i8, i8, i8)) -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(year, Month::March, 5).unwrap(),
            Time::from_hms_nano(14, 7, 9, nanos).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(offset.0, offset.1, offset.2).unwrap())
    }

    fn canonical_cases() -> Vec<(OffsetDateTime, &'static str)> {
        vec![
            (at(2026, 0, (0, 0, 0)), "2026-03-05T14:07:09Z"),
            (at(2026, 0, (3, 0, 0)), "2026-03-05T14:07:09+03:00"),
            (at(2026, 0, (-5, -30, 0)), "2026-03-05T14:07:09-05:30"),
            (at(2026, 500_000_000, (0, 0, 0)), "2026-03-05T14:07:09.5Z"),
            (at(2026, 1, (0, 0, 0)), "2026-03-05T14:07:09.000000001Z"),
            (at(0, 0, (0, 0, 0)), "0000-03-05T14:07:09Z"),
            (at(9999, 0, (0, 0, 0)), "9999-03-05T14:07:09Z"),
            (at(-1, 0, (0, 0, 0)), "-0001-03-05 14:07:09.0 +00:00:00"),
            (at(2026, 0, (1, 0, 30)), "2026-03-05 14:07:09.0 +01:00:30"),
        ]
    }

    #[test]
    fn to_text_renders_rfc3339_and_falls_back_to_a_non_empty_form_rfc3339_cannot_express() {
        for (value, expected) in canonical_cases() {
            assert_eq!(datetime_to_text(&value), expected, "value {value:?}");
        }
    }

    #[test]
    fn every_forge_types_renderer_writes_a_datetime_as_the_canonical_text() {
        for (value, expected) in canonical_cases() {
            let variant = Variant::Datetime(value);
            assert_eq!(variant.to_string(), expected, "Variant Display");
            assert_eq!(
                variant.to_plain_json(),
                serde_json::Value::String(expected.to_owned()),
                "Variant::to_plain_json"
            );
            assert_eq!(
                DisclosedValue::Datetime(value).to_string(),
                expected,
                "DisclosedValue Display (run disclosure)"
            );
        }
    }

    #[test]
    fn from_text_reads_tolerated_spellings_after_trimming() {
        for (raw, expected) in [
            ("2026-03-05T14:07:09Z", at(2026, 0, (0, 0, 0))),
            ("  2026-03-05T14:07:09Z\n", at(2026, 0, (0, 0, 0))),
            ("2026-03-05t14:07:09z", at(2026, 0, (0, 0, 0))),
            ("2026-03-05 14:07:09Z", at(2026, 0, (0, 0, 0))),
            ("2026-03-05T14:07:09.000Z", at(2026, 0, (0, 0, 0))),
            ("2026-03-05T14:07:09+03:00", at(2026, 0, (3, 0, 0))),
        ] {
            let parsed = datetime_from_text(raw).unwrap();
            assert_eq!(
                (parsed, parsed.offset()),
                (expected, expected.offset()),
                "input {raw:?}"
            );
        }
    }

    #[test]
    fn from_text_rejects_text_that_does_not_name_an_instant_with_an_offset() {
        for raw in [
            "",
            "   ",
            "2026-03-05T14:07:09",
            "2026-03-05",
            "14:07:09Z",
            "1772719629",
            "tomorrow",
        ] {
            assert_eq!(datetime_from_text(raw), None, "input {raw:?}");
        }
    }

    #[test]
    fn from_text_reads_back_what_to_text_wrote_with_the_offset_intact() {
        for (value, _) in canonical_cases().into_iter().filter(|(v, _)| {
            (0..=9999).contains(&v.year()) && v.offset().seconds_past_minute() == 0
        }) {
            let back = datetime_from_text(&datetime_to_text(&value)).unwrap();
            assert_eq!((back, back.offset()), (value, value.offset()));
        }
    }
}
