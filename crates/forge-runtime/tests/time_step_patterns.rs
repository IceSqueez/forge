#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]
use forge_events::test_support::NullPublisher;
use std::process::Command;

use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::{CoreTimeFormatRunner, CoreTimeNowRunner, CoreTimeParseRunner};
use forge_types::{ArgStack, EventId, NamesLanguage, SubActionConfig, SubActionOutcome, Variant};
use time::{Date, Month, OffsetDateTime, UtcOffset};

const CHILD_MARKER: &str = "FORGE_TIME_STEP_PATTERNS_CHILD";

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
    assert!(
        output.status.success(),
        "child run of {test_name} in TZ={zone} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

macro_rules! zone_test {
    ($name:ident, $zone:literal, $body:block) => {
        #[tokio::test]
        async fn $name() {
            if in_child_zone(stringify!($name), $zone) $body
        }
    };
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
    run_ok_in(runner, config, NamesLanguage::En).await
}

async fn run_ok_in(
    runner: &dyn SubActionRunner,
    config: &SubActionConfig,
    language: NamesLanguage,
) -> ArgStack {
    let stack = ArgStack::new().with_names_language(language);
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
    formatted_in(config, NamesLanguage::En).await
}

async fn formatted_in(config: &SubActionConfig, language: NamesLanguage) -> String {
    run_ok_in(&CoreTimeFormatRunner, config, language)
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

zone_test!(
    format_pattern_follows_the_system_zone_when_utc_is_off_and_ignores_it_when_on,
    "Europe/Kyiv",
    {
        for (utc, expected) in [(false, "15:00 +03:00"), (true, "12:00 +00:00")] {
            assert_eq!(
                formatted(&format_cfg("HH:mm xxx", Some(utc))).await,
                expected,
                "utc {utc}"
            );
        }
    }
);

#[tokio::test]
async fn format_pattern_writes_names_in_the_language_its_arg_stack_carries() {
    for (language, expected) in [
        (NamesLanguage::Uk, "15 жовтня, четвер"),
        (NamesLanguage::En, "15 October, Thursday"),
    ] {
        assert_eq!(
            formatted_in(&format_cfg("d MMMM, EEEE", Some(true)), language).await,
            expected,
            "{language:?}"
        );
    }
}

zone_test!(
    parse_pattern_reads_text_in_the_system_zone_when_utc_is_off_and_keeps_its_offset,
    "Europe/Kyiv",
    {
        let dt = parsed(&cfg(&[
            ("source", s("15.10.2026 15:00")),
            ("format", s("dd.MM.yyyy HH:mm")),
            ("utc", Variant::Bool(false)),
        ]))
        .await;
        assert_eq!(dt, mid_october_noon_utc());
        assert_eq!(dt.offset(), UtcOffset::from_hms(3, 0, 0).unwrap());
    }
);

zone_test!(
    parse_pattern_reads_text_as_utc_when_utc_is_on_in_any_zone,
    "Europe/Kyiv",
    {
        let dt = parsed(&cfg(&[
            ("source", s("15.10.2026 12:00")),
            ("format", s("dd.MM.yyyy HH:mm")),
            ("utc", Variant::Bool(true)),
        ]))
        .await;
        assert_eq!(dt, mid_october_noon_utc());
        assert_eq!(dt.offset(), UtcOffset::UTC);
    }
);

zone_test!(
    custom_now_follows_the_system_zone_when_utc_is_off,
    "Asia/Tokyo",
    {
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
);

#[test]
fn parse_and_now_default_configs_validate() {
    let runners: [&dyn SubActionRunner; 2] = [&CoreTimeParseRunner, &CoreTimeNowRunner];
    for runner in runners {
        assert!(
            runner.validate_config(&runner.default_config()).is_ok(),
            "{}",
            runner.id()
        );
    }
}

#[test]
fn a_fresh_format_step_fails_validation_on_its_empty_source() {
    let problem = CoreTimeFormatRunner
        .validate_config(&CoreTimeFormatRunner.default_config())
        .unwrap_err()
        .into_config_problem();
    assert_eq!(
        problem,
        forge_registry::ConfigProblem::field("source", forge_registry::ConfigReason::Required)
    );
}
