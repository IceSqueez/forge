#![allow(clippy::expect_used)]

use std::ops::Deref;

use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

pub(crate) struct Sandboxed<T> {
    inner: T,
    _media_root: TempDir,
}

impl<T> Sandboxed<T> {
    pub(crate) fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
        Sandboxed {
            inner: wrap(self.inner),
            _media_root: self._media_root,
        }
    }
}

impl<T> Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub(crate) async fn sandboxed_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = SqliteBackend::open_for_test(":memory:", key, media_root.path().to_owned(), None)
        .await
        .expect("an in-memory backend");
    Sandboxed {
        inner: backend,
        _media_root: media_root,
    }
}

pub(crate) mod log_capture {
    #![allow(clippy::unwrap_used)]

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span;
    use tracing::subscriber::Interest;
    use tracing::{Level, Subscriber};

    #[derive(Clone)]
    pub(crate) struct CapturedLine {
        pub(crate) target: String,
        pub(crate) level: Level,
        fields: BTreeMap<String, String>,
    }

    impl CapturedLine {
        pub(crate) fn field(&self, name: &str) -> &str {
            self.fields
                .get(name)
                .map(String::as_str)
                .unwrap_or_default()
        }

        pub(crate) fn mentions(&self, needle: &str) -> bool {
            self.fields.values().any(|value| value.contains(needle))
        }
    }

    #[derive(Default)]
    struct FieldCollector(BTreeMap<String, String>);

    impl Visit for FieldCollector {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_owned(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_owned(), value.to_owned());
        }
    }

    struct CaptureSubscriber {
        lines: Arc<Mutex<Vec<CapturedLine>>>,
        max: Level,
    }

    impl Subscriber for CaptureSubscriber {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() <= self.max
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut collector = FieldCollector::default();
            event.record(&mut collector);
            self.lines.lock().unwrap().push(CapturedLine {
                target: event.metadata().target().to_owned(),
                level: *event.metadata().level(),
                fields: collector.0,
            });
        }

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    struct InterestFloor;

    impl Subscriber for InterestFloor {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            false
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, _: &tracing::Event<'_>) {}

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    fn install_interest_floor() {
        static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        INSTALLED.get_or_init(|| {
            let _ = tracing::subscriber::set_global_default(InterestFloor);
            tracing::callsite::rebuild_interest_cache();
        });
    }

    pub(crate) fn capture(max: Level, body: impl FnOnce()) -> Vec<CapturedLine> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        install_interest_floor();
        tracing::subscriber::with_default(
            CaptureSubscriber {
                lines: Arc::clone(&lines),
                max,
            },
            || {
                tracing::callsite::rebuild_interest_cache();
                body();
            },
        );
        lines.lock().unwrap().clone()
    }

    pub(crate) fn on_target<'a>(lines: &'a [CapturedLine], target: &str) -> Vec<&'a CapturedLine> {
        lines.iter().filter(|line| line.target == target).collect()
    }
}
