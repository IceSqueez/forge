#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]

use std::sync::Arc;

use forge_events::test_support::NullPublisher;
use forge_registry::{CancelSignal, SubActionRegistry};
use forge_runtime::{ChainEngine, ConditionGate, Config, CoreTimeFormatRunner, DateNamesHandle};
use forge_types::{ArgStack, EventId, NamesLanguage, SubActionConfig, SubActionStep, Variant};
use time::{Date, Month, OffsetDateTime};

const ROUNDS: usize = 500;

fn mid_october_noon_utc() -> OffsetDateTime {
    Date::from_calendar_date(2026, Month::October, 15)
        .unwrap()
        .with_hms(12, 0, 0)
        .unwrap()
        .assume_utc()
}

fn engine(date_names: DateNamesHandle) -> Arc<ChainEngine> {
    let config = Config::default();
    let gate = Arc::new(ConditionGate::new(&config));
    let mut registry = SubActionRegistry::new();
    registry.register(Box::new(CoreTimeFormatRunner)).unwrap();
    Arc::new(ChainEngine::new(
        Arc::new(registry),
        Arc::new(NullPublisher),
        gate,
        date_names,
        config,
    ))
}

fn format_step() -> SubActionStep {
    let config = SubActionConfig::from([
        (
            "source".to_owned(),
            Variant::Datetime(mid_october_noon_utc()),
        ),
        (
            "format_string".to_owned(),
            Variant::String("MMMM".to_owned()),
        ),
        ("utc".to_owned(), Variant::Bool(true)),
    ]);
    SubActionStep {
        kind_id: "core.time.format".to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

async fn month_name(engine: &Arc<ChainEngine>) -> String {
    let run = engine
        .run_sequential(
            &[format_step()],
            &ArgStack::new(),
            EventId::new(),
            &CancelSignal::new(),
        )
        .await;
    run.arg_stack
        .get("time.formatted")
        .and_then(Variant::as_str)
        .expect("the format step wrote its output")
        .to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_engines_in_different_languages_run_in_parallel_without_a_leak() {
    let english = engine(DateNamesHandle::new(NamesLanguage::En));
    let ukrainian = engine(DateNamesHandle::new(NamesLanguage::Uk));

    let english_run = tokio::spawn(async move {
        for _ in 0..ROUNDS {
            assert_eq!(month_name(&english).await, "October");
        }
    });
    let ukrainian_run = tokio::spawn(async move {
        for _ in 0..ROUNDS {
            assert_eq!(month_name(&ukrainian).await, "жовтня");
        }
    });

    english_run.await.unwrap();
    ukrainian_run.await.unwrap();
}

#[tokio::test]
async fn a_language_switched_through_the_handle_reaches_the_next_run() {
    let date_names = DateNamesHandle::new(NamesLanguage::En);
    let engine = engine(date_names.clone());

    for (language, expected) in [
        (NamesLanguage::Uk, "жовтня"),
        (NamesLanguage::En, "October"),
        (NamesLanguage::Uk, "жовтня"),
    ] {
        date_names.set_language(language);
        assert_eq!(month_name(&engine).await, expected, "{language:?}");
    }
}
