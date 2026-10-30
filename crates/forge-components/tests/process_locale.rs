#![allow(clippy::unwrap_used)]

use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, PoisonError};

use forge_components::{
    CatalogLanguage, fmt_number, install_process_language, install_thread_language,
    resolve_in_language, tr,
};

static PROCESS_STATE: Mutex<()> = Mutex::new(());

const KIT_KEY: &str = "widget_date_month_january";
const DESKTOP_EN: &str = include_str!("../../forge-desktop/locales/en/main.ftl");
const DESKTOP_UK: &str = include_str!("../../forge-desktop/locales/uk/main.ftl");

type Localized = (String, String);
type Runner = fn(fn() -> Localized) -> Localized;

fn serialized() -> MutexGuard<'static, ()> {
    PROCESS_STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn localized() -> Localized {
    (tr!(KIT_KEY), fmt_number(1234.5, 1))
}

fn expected(language: CatalogLanguage) -> Localized {
    let (month, number) = match language {
        CatalogLanguage::En => ("January", "1,234.5"),
        CatalogLanguage::Uk => ("Січень", "1 234,5"),
    };
    (month.to_owned(), number.to_owned())
}

fn on_std_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::spawn(body).join().unwrap()
}

fn on_tokio_worker<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    runtime
        .block_on(runtime.spawn(async move { body() }))
        .unwrap()
}

#[test]
fn process_language_resolves_text_and_numbers_on_tokio_workers_and_std_threads() {
    let _guard = serialized();
    let runners: [(&str, Runner); 2] = [
        ("tokio worker", |body| on_tokio_worker(body)),
        ("std thread", |body| on_std_thread(body)),
    ];
    for language in [CatalogLanguage::En, CatalogLanguage::Uk] {
        install_process_language(language, &[]);
        for (runner_name, run) in runners {
            assert_eq!(
                run(localized),
                expected(language),
                "{language:?} on {runner_name}"
            );
        }
    }
}

#[test]
fn a_language_switch_is_seen_by_a_thread_that_was_already_running() {
    let _guard = serialized();
    let (ask, asked) = mpsc::channel::<()>();
    let (answer, answers) = mpsc::channel::<Localized>();
    let worker = std::thread::spawn(move || {
        while asked.recv().is_ok() {
            answer.send(localized()).unwrap();
        }
    });

    let mut seen = Vec::new();
    for language in [CatalogLanguage::Uk, CatalogLanguage::En] {
        install_process_language(language, &[]);
        ask.send(()).unwrap();
        seen.push(answers.recv().unwrap());
    }
    drop(ask);
    worker.join().unwrap();

    assert_eq!(
        seen,
        vec![expected(CatalogLanguage::Uk), expected(CatalogLanguage::En)]
    );
}

#[test]
fn a_thread_override_applies_only_to_the_thread_that_installed_it() {
    let _guard = serialized();
    install_process_language(CatalogLanguage::Uk, &[]);

    let overridden = on_std_thread(|| {
        install_thread_language(CatalogLanguage::En, &[]);
        localized()
    });
    let other = on_tokio_worker(localized);

    assert_eq!(
        (overridden, other),
        (expected(CatalogLanguage::En), expected(CatalogLanguage::Uk))
    );
}

#[test]
fn resolve_in_language_changes_neither_the_thread_nor_the_process_language() {
    let _guard = serialized();
    install_process_language(CatalogLanguage::En, &[]);

    let overridden = on_std_thread(|| {
        install_thread_language(CatalogLanguage::Uk, &[]);
        let one_off = resolve_in_language(CatalogLanguage::En, &[], KIT_KEY);
        (one_off, localized())
    });
    let process = on_std_thread(|| {
        let one_off = resolve_in_language(CatalogLanguage::Uk, &[], KIT_KEY);
        (one_off, localized())
    });

    assert_eq!(
        (overridden, process),
        (
            ("January".to_owned(), expected(CatalogLanguage::Uk)),
            ("Січень".to_owned(), expected(CatalogLanguage::En)),
        )
    );
}

#[test]
fn an_app_source_wins_over_the_kit_catalog_on_a_duplicate_message_id() {
    let _guard = serialized();
    install_process_language(
        CatalogLanguage::En,
        &["widget_date_month_january = Month one\n"],
    );

    assert_eq!(on_tokio_worker(|| tr!(KIT_KEY)), "Month one");
}

#[test]
fn discord_test_post_body_resolves_on_a_tokio_worker_instead_of_the_raw_key() {
    let _guard = serialized();
    let mut resolved = Vec::new();
    for (language, app_source) in [
        (CatalogLanguage::En, DESKTOP_EN),
        (CatalogLanguage::Uk, DESKTOP_UK),
    ] {
        install_process_language(language, &[app_source]);
        resolved.push(on_tokio_worker(|| tr!("discord_test_content")));
    }

    assert_eq!(
        resolved,
        vec![
            "Test message from forge.".to_owned(),
            "Тестове повідомлення з forge.".to_owned(),
        ]
    );
}

#[cfg(debug_assertions)]
mod missing_key_warnings {
    use std::rc::Rc;
    use std::sync::Arc;

    use fluent::{FluentBundle, FluentResource};
    use forge_components::{catalog_source, install_bundle};

    use super::*;

    const KEY_NOT_FOUND: &str = "tr!: key not found in active bundle";

    type Warning = (Option<String>, Option<String>);

    #[derive(Default)]
    struct WarnFields {
        key: Option<String>,
        message: Option<String>,
    }

    impl tracing::field::Visit for WarnFields {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            if field.name() == "message" {
                self.message = Some(format!("{value:?}"));
            }
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "key" {
                self.key = Some(value.to_owned());
            }
        }
    }

    struct WarnCapture(Arc<Mutex<Vec<Warning>>>);

    impl tracing::Subscriber for WarnCapture {
        fn register_callsite(
            &self,
            _: &'static tracing::Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            tracing::subscriber::Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() == tracing::Level::WARN
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut fields = WarnFields::default();
            event.record(&mut fields);
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((fields.key, fields.message));
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn warnings_on_a_new_thread(body: impl FnOnce() + Send + 'static) -> Vec<Warning> {
        on_std_thread(move || {
            let lines = Arc::new(Mutex::new(Vec::new()));
            let capture = WarnCapture(Arc::clone(&lines));
            tracing::subscriber::with_default(capture, || {
                tracing::callsite::rebuild_interest_cache();
                body();
            });
            lines.lock().unwrap_or_else(PoisonError::into_inner).clone()
        })
    }

    fn miss(key: &'static str) {
        assert_eq!(tr!(key), key);
    }

    fn not_found(key: &str) -> Warning {
        (Some(key.to_owned()), Some(KEY_NOT_FOUND.to_owned()))
    }

    fn kit_bundle(language: CatalogLanguage) -> Rc<FluentBundle<FluentResource>> {
        let mut bundle = FluentBundle::new(vec!["en-US".parse().unwrap()]);
        bundle
            .add_resource(FluentResource::try_new(catalog_source(language).to_owned()).unwrap())
            .unwrap();
        Rc::new(bundle)
    }

    #[test]
    fn a_process_missing_key_warns_once_across_all_threads() {
        let _guard = serialized();
        install_process_language(CatalogLanguage::En, &[]);

        let first = warnings_on_a_new_thread(|| {
            miss("absent_across_threads");
            miss("absent_across_threads");
        });
        let second = warnings_on_a_new_thread(|| miss("absent_across_threads"));

        assert_eq!(
            (first, second),
            (vec![not_found("absent_across_threads")], vec![])
        );
    }

    #[test]
    fn installing_a_process_language_lets_an_already_warned_key_warn_again() {
        let _guard = serialized();
        install_process_language(CatalogLanguage::En, &[]);
        let before = warnings_on_a_new_thread(|| miss("absent_across_installs"));

        install_process_language(CatalogLanguage::Uk, &[]);
        let after = warnings_on_a_new_thread(|| miss("absent_across_installs"));

        assert_eq!(
            (before, after),
            (
                vec![not_found("absent_across_installs")],
                vec![not_found("absent_across_installs")],
            )
        );
    }

    #[test]
    fn a_thread_override_dedupes_missing_keys_apart_from_the_process_and_resets_on_install_bundle()
    {
        let _guard = serialized();
        install_process_language(CatalogLanguage::En, &[]);

        let process = warnings_on_a_new_thread(|| miss("absent_per_scope"));
        let overridden = warnings_on_a_new_thread(|| {
            install_thread_language(CatalogLanguage::Uk, &[]);
            miss("absent_per_scope");
            miss("absent_per_scope");
            install_bundle(kit_bundle(CatalogLanguage::Uk));
            miss("absent_per_scope");
        });

        assert_eq!(
            (process, overridden),
            (
                vec![not_found("absent_per_scope")],
                vec![not_found("absent_per_scope"), not_found("absent_per_scope")],
            )
        );
    }
}
