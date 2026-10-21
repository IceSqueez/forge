use time::{Duration, OffsetDateTime};

pub(crate) fn next_due(
    last_success: Option<OffsetDateTime>,
    interval: Duration,
    now: OffsetDateTime,
) -> OffsetDateTime {
    match last_success {
        Some(last) if last <= now => last + interval,
        _ => now,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use time::{Date, Duration, Month, OffsetDateTime};

    use super::next_due;

    fn noon() -> OffsetDateTime {
        Date::from_calendar_date(2026, Month::October, 21)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
    }

    #[test]
    fn next_due_follows_the_last_success_and_falls_back_to_now() {
        let now = noon();
        let interval = Duration::minutes(60);
        for (label, last, expected) in [
            ("never ran", None, now),
            (
                "ran recently",
                Some(now - Duration::minutes(10)),
                now + Duration::minutes(50),
            ),
            ("ran exactly now", Some(now), now + interval),
            (
                "missed many slots while closed",
                Some(now - Duration::days(3)),
                now - Duration::days(3) + interval,
            ),
            ("clock moved back", Some(now + Duration::days(1)), now),
        ] {
            assert_eq!(next_due(last, interval, now), expected, "{label}");
        }
    }
}
