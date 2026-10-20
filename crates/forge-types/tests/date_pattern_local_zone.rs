#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use forge_types::{DatePattern, NamesLanguage, PatternZone};
use time::{Date, Month, OffsetDateTime, UtcOffset};

const CHILD_MARKER: &str = "FORGE_DATE_PATTERN_ZONE_CHILD";
const FULL_PATTERN: &str = "yyyy-MM-dd HH:mm:ss xxx";

fn in_child_zone(test_name: &str, zone: &str) -> bool {
    if std::env::var_os(CHILD_MARKER).is_some() {
        return true;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--test-threads=1", "--nocapture"])
        .env(CHILD_MARKER, "1")
        .env("TZ", zone)
        .output()
        .expect("re-run the test binary in a child process");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child run of {test_name} in TZ={zone} failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

fn utc(year: i32, month: Month, day: u8, hms: (u8, u8, u8)) -> OffsetDateTime {
    Date::from_calendar_date(year, month, day)
        .unwrap()
        .with_hms(hms.0, hms.1, hms.2)
        .unwrap()
        .assume_offset(UtcOffset::UTC)
}

fn local(pattern: &str, value: OffsetDateTime) -> String {
    DatePattern::parse(pattern)
        .unwrap()
        .format(value, PatternZone::Local, NamesLanguage::En)
}

#[test]
fn local_zone_follows_kyiv_fall_back_on_last_october_sunday() {
    if !in_child_zone(
        "local_zone_follows_kyiv_fall_back_on_last_october_sunday",
        "Europe/Kyiv",
    ) {
        return;
    }
    for (instant, expected) in [
        (
            utc(2026, Month::October, 25, (0, 59, 59)),
            "2026-10-25 03:59:59 +03:00",
        ),
        (
            utc(2026, Month::October, 25, (1, 0, 0)),
            "2026-10-25 03:00:00 +02:00",
        ),
        (
            utc(2026, Month::October, 24, (22, 30, 0)),
            "2026-10-25 01:30:00 +03:00",
        ),
    ] {
        assert_eq!(local(FULL_PATTERN, instant), expected, "{instant}");
    }
}

#[test]
fn local_zone_renders_a_negative_half_hour_offset() {
    if !in_child_zone(
        "local_zone_renders_a_negative_half_hour_offset",
        "America/St_Johns",
    ) {
        return;
    }
    assert_eq!(
        local(FULL_PATTERN, utc(2026, Month::January, 15, (2, 0, 0))),
        "2026-01-14 22:30:00 -03:30"
    );
}

#[test]
fn local_zone_ignores_the_input_offset() {
    if !in_child_zone("local_zone_ignores_the_input_offset", "Europe/Kyiv") {
        return;
    }
    let tokyo =
        utc(2026, Month::January, 15, (12, 0, 0)).to_offset(UtcOffset::from_hms(9, 0, 0).unwrap());
    assert_eq!(local(FULL_PATTERN, tokyo), "2026-01-15 14:00:00 +02:00");
}

#[test]
fn utc_zone_ignores_the_system_zone() {
    if !in_child_zone("utc_zone_ignores_the_system_zone", "Europe/Kyiv") {
        return;
    }
    let rendered = DatePattern::parse(FULL_PATTERN).unwrap().format(
        utc(2026, Month::July, 1, (12, 0, 0)),
        PatternZone::Utc,
        NamesLanguage::En,
    );
    assert_eq!(rendered, "2026-07-01 12:00:00 +00:00");
}

#[test]
fn local_zone_falls_back_to_utc_outside_the_supported_range() {
    if !in_child_zone(
        "local_zone_falls_back_to_utc_outside_the_supported_range",
        "Europe/Kyiv",
    ) {
        return;
    }
    for (instant, expected) in [
        (
            utc(9999, Month::December, 30, (21, 0, 0)),
            "9999-12-30 23:00:00 +02:00",
        ),
        (
            utc(9999, Month::December, 31, (23, 0, 0)),
            "9999-12-31 23:00:00 +00:00",
        ),
        (
            utc(-9999, Month::January, 1, (0, 0, 0)),
            "-9999-01-01 00:00:00 +00:00",
        ),
    ] {
        assert_eq!(local(FULL_PATTERN, instant), expected, "{instant}");
    }
}
