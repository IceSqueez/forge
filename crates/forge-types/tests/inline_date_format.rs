#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};

use forge_types::{ArgStack, NamesLanguage, Variant, set_current_names_language};
use time::{Date, Month, OffsetDateTime};

const CHILD_MARKER: &str = "FORGE_INLINE_DATE_FORMAT_CHILD";

static NAMES_LANGUAGE_LOCK: Mutex<()> = Mutex::new(());

fn names_language(language: NamesLanguage) -> MutexGuard<'static, ()> {
    let guard = NAMES_LANGUAGE_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    set_current_names_language(language);
    guard
}

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

fn mid_october_noon_utc() -> OffsetDateTime {
    Date::from_calendar_date(2026, Month::October, 15)
        .unwrap()
        .with_hms(12, 0, 0)
        .unwrap()
        .assume_utc()
}

fn stack_at(at: OffsetDateTime) -> ArgStack {
    ArgStack::new().set("at".to_string(), Variant::Datetime(at))
}

#[test]
fn interpolate_formats_a_datetime_in_the_system_local_zone() {
    if !in_child_zone(
        "interpolate_formats_a_datetime_in_the_system_local_zone",
        "Europe/Kyiv",
    ) {
        return;
    }
    assert_eq!(
        stack_at(mid_october_noon_utc()).interpolate("at %at:HH:mm xxx%!"),
        "at 15:00 +03:00!"
    );
}

#[test]
fn interpolate_uses_ukrainian_genitive_and_nominative_month_names_when_set_to_uk() {
    let _language = names_language(NamesLanguage::Uk);
    assert_eq!(
        stack_at(mid_october_noon_utc()).interpolate("%at:d MMMM% / %at:LLLL%"),
        "15 жовтня / жовтень"
    );
}

#[test]
fn interpolate_uses_english_month_names_when_set_to_en() {
    let _language = names_language(NamesLanguage::En);
    assert_eq!(
        stack_at(mid_october_noon_utc()).interpolate("%at:MMMM%"),
        "October"
    );
}
