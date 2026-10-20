#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreTimeFormatRunner, CoreTimeNowRunner, CoreTimeParseRunner,
};
use forge_types::{
    ArgStack, EventId, NamesLanguage, SubActionConfig, SubActionOutcome, Variant,
    set_current_names_language,
};
use time::{Date, Month, OffsetDateTime, UtcOffset};
use tokio::sync::{Mutex, MutexGuard};

const CHILD_MARKER: &str = "FORGE_TIME_STEP_PATTERNS_CHILD";

static NAMES_LANGUAGE_LOCK: Mutex<()> = Mutex::const_new(());

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

async fn names_language(language: NamesLanguage) -> MutexGuard<'static, ()> {
    let guard = NAMES_LANGUAGE_LOCK.lock().await;
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

fn cfg(entries: &[(&str, Variant)]) -> SubActionConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

async fn run_ok(runner: &dyn SubActionRunner, config: &SubActionConfig) -> ArgStack {
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, out) = runner.execute(config, &ctx).await;
    assert!(
        matches!(telemetry.outcome, SubActionOutcome::Success),
        "{:?}",
        telemetry.outcome
    );
    out.unwrap()
}

async fn formatted(config: &SubActionConfig) -> String {
    run_ok(&CoreTimeFormatRunner, config)
        .await
        .get("time.formatted")
        .and_then(Variant::as_str)
        .unwrap()
        .to_owned()
}

async fn parsed(config: &SubActionConfig) -> OffsetDateTime {
    *run_ok(&CoreTimeParseRunner, config)
        .await
        .get("time.parsed")
        .and_then(Variant::as_datetime)
        .unwrap()
}

fn format_cfg(pattern: &str, utc: Option<bool>) -> SubActionConfig {
    let mut config = cfg(&[
        ("source", Variant::Datetime(mid_october_noon_utc())),
        ("format_string", s(pattern)),
    ]);
    if let Some(utc) = utc {
        config.insert("utc".to_owned(), Variant::Bool(utc));
    }
    config
}

#[tokio::test]
async fn format_pattern_follows_the_system_zone_when_utc_is_off_and_ignores_it_when_on() {
    if !in_child_zone(
        "format_pattern_follows_the_system_zone_when_utc_is_off_and_ignores_it_when_on",
        "Europe/Kyiv",
    ) {
        return;
    }
    for (utc, expected) in [(false, "15:00 +03:00"), (true, "12:00 +00:00")] {
        assert_eq!(
            formatted(&format_cfg("HH:mm xxx", Some(utc))).await,
            expected,
            "utc {utc}"
        );
    }
}

#[tokio::test]
async fn format_pattern_uses_the_current_names_language() {
    let _language = names_language(NamesLanguage::Uk).await;
    assert_eq!(
        formatted(&format_cfg("d MMMM, EEEE", Some(true))).await,
        "15 жовтня, четвер"
    );
}

#[tokio::test]
async fn legacy_format_without_utc_key_renders_utc_english_in_any_zone_and_language() {
    if !in_child_zone(
        "legacy_format_without_utc_key_renders_utc_english_in_any_zone_and_language",
        "Europe/Kyiv",
    ) {
        return;
    }
    let _language = names_language(NamesLanguage::Uk).await;
    let legacy = "[weekday], [day] [month repr:long] [hour]:[minute] [period]";
    let old = mid_october_noon_utc()
        .format(&time::format_description::parse_borrowed::<2>(legacy).unwrap())
        .unwrap();
    assert_eq!(formatted(&format_cfg(legacy, None)).await, old);
}

#[tokio::test]
async fn legacy_format_with_utc_off_follows_the_system_zone() {
    if !in_child_zone(
        "legacy_format_with_utc_off_follows_the_system_zone",
        "Europe/Kyiv",
    ) {
        return;
    }
    assert_eq!(
        formatted(&format_cfg("[hour]:[minute]", Some(false))).await,
        "15:00"
    );
}

#[tokio::test]
async fn parse_pattern_reads_text_in_the_system_zone_when_utc_is_off_and_keeps_its_offset() {
    if !in_child_zone(
        "parse_pattern_reads_text_in_the_system_zone_when_utc_is_off_and_keeps_its_offset",
        "Europe/Kyiv",
    ) {
        return;
    }
    let dt = parsed(&cfg(&[
        ("source", s("15.10.2026 15:00")),
        ("format", s("dd.MM.yyyy HH:mm")),
        ("utc", Variant::Bool(false)),
    ]))
    .await;
    assert_eq!(dt, mid_october_noon_utc());
    assert_eq!(dt.offset(), UtcOffset::from_hms(3, 0, 0).unwrap());
}

#[tokio::test]
async fn parse_pattern_reads_text_as_utc_when_utc_is_on_in_any_zone() {
    if !in_child_zone(
        "parse_pattern_reads_text_as_utc_when_utc_is_on_in_any_zone",
        "Europe/Kyiv",
    ) {
        return;
    }
    let dt = parsed(&cfg(&[
        ("source", s("15.10.2026 12:00")),
        ("format", s("dd.MM.yyyy HH:mm")),
        ("utc", Variant::Bool(true)),
    ]))
    .await;
    assert_eq!(dt, mid_october_noon_utc());
    assert_eq!(dt.offset(), UtcOffset::UTC);
}

#[tokio::test]
async fn legacy_parse_without_utc_key_reads_text_as_utc_in_any_zone() {
    if !in_child_zone(
        "legacy_parse_without_utc_key_reads_text_as_utc_in_any_zone",
        "Europe/Kyiv",
    ) {
        return;
    }
    let dt = parsed(&cfg(&[
        ("source", s("2026-10-15 12:00:00")),
        ("format", s("[year]-[month]-[day] [hour]:[minute]:[second]")),
    ]))
    .await;
    assert_eq!(dt, mid_october_noon_utc());
}

#[tokio::test]
async fn custom_now_follows_the_system_zone_when_utc_is_off() {
    if !in_child_zone(
        "custom_now_follows_the_system_zone_when_utc_is_off",
        "Asia/Tokyo",
    ) {
        return;
    }
    let out = run_ok(
        &CoreTimeNowRunner,
        &cfg(&[
            ("format", s("custom")),
            ("custom_format_string", s("xxx")),
            ("utc", Variant::Bool(false)),
        ]),
    )
    .await;
    assert_eq!(
        out.get("time.formatted").and_then(Variant::as_str),
        Some("+09:00")
    );
}

#[test]
fn every_time_step_default_config_validates() {
    let runners: [&dyn SubActionRunner; 3] = [
        &CoreTimeFormatRunner,
        &CoreTimeParseRunner,
        &CoreTimeNowRunner,
    ];
    for runner in runners {
        assert!(
            runner.validate_config(&runner.default_config()).is_ok(),
            "{}",
            runner.id()
        );
    }
}
