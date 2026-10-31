#![expect(clippy::unwrap_used, reason = "helpers outside #[test] functions")]

mod common;

use forge_types::{ArgStack, NamesLanguage, Variant};

use common::in_child_zone;
use time::{Date, Month, OffsetDateTime};

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
fn interpolate_uses_ukrainian_genitive_and_nominative_month_names_on_a_uk_stack() {
    assert_eq!(
        stack_at(mid_october_noon_utc())
            .with_names_language(NamesLanguage::Uk)
            .interpolate("%at:d MMMM% / %at:LLLL%"),
        "15 жовтня / жовтень"
    );
}

#[test]
fn interpolate_defaults_to_english_month_names() {
    assert_eq!(
        stack_at(mid_october_noon_utc()).interpolate("%at:MMMM%"),
        "October"
    );
}

#[test]
fn stacks_in_different_languages_interpolate_side_by_side_without_a_leak() {
    let at = mid_october_noon_utc();
    let english = std::thread::spawn(move || {
        (0..1_000)
            .map(|_| stack_at(at).interpolate("%at:MMMM%"))
            .all(|text| text == "October")
    });
    let ukrainian = std::thread::spawn(move || {
        (0..1_000)
            .map(|_| {
                stack_at(at)
                    .with_names_language(NamesLanguage::Uk)
                    .interpolate("%at:LLLL%")
            })
            .all(|text| text == "жовтень")
    });
    assert!(english.join().unwrap());
    assert!(ukrainian.join().unwrap());
}
