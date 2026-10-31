#![expect(clippy::unwrap_used, reason = "helpers outside #[test] functions")]

mod common;

use forge_types::{ArgStack, NamesLanguage, RecoverMutex, Variant, set_current_names_language};
use std::sync::{Mutex, MutexGuard};

use common::in_child_zone;
use time::{Date, Month, OffsetDateTime};

static NAMES_LANGUAGE_LOCK: Mutex<()> = Mutex::new(());

fn names_language(language: NamesLanguage) -> MutexGuard<'static, ()> {
    let guard = NAMES_LANGUAGE_LOCK.lock_recover();
    set_current_names_language(language);
    guard
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
