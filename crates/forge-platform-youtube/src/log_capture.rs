//! Thread-local tracing capture used by the log-hygiene tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tracing::Level;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::subscriber::Interest;

#[derive(Clone, Debug)]
pub(crate) struct CapturedLine {
    pub(crate) target: String,
    pub(crate) level: Level,
    pub(crate) fields: BTreeMap<String, String>,
}

impl CapturedLine {
    pub(crate) fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    pub(crate) fn message(&self) -> &str {
        self.field("message").unwrap_or_default()
    }

    pub(crate) fn field_names(&self) -> Vec<&str> {
        self.fields.keys().map(String::as_str).collect()
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

// Why: `register_callsite` answers `sometimes` on purpose - a cached `always` from another
// capture running in parallel would hand a TRACE line to a DEBUG-only assertion.
struct CaptureSubscriber {
    lines: Arc<Mutex<Vec<CapturedLine>>>,
    max: Level,
}

impl tracing::Subscriber for CaptureSubscriber {
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
        self.lines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CapturedLine {
                target: event.metadata().target().to_owned(),
                level: *event.metadata().level(),
                fields: collector.0,
            });
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

/// Drives `future` to completion on a current-thread runtime owned by this call, with every
/// line up to `max` captured. Runtime and subscriber share the calling thread precisely so the
/// thread-local subscriber sees the work; callers must therefore be plain `#[test]` fns.
pub(crate) fn capture_blocking<F: Future>(max: Level, future: F) -> (F::Output, Vec<CapturedLine>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let subscriber = CaptureSubscriber {
        lines: Arc::clone(&lines),
        max,
    };
    let output = tracing::subscriber::with_default(subscriber, || {
        tracing::callsite::rebuild_interest_cache();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime must build");
        runtime.block_on(future)
    });
    let captured = lines.lock().unwrap_or_else(|e| e.into_inner()).clone();
    (output, captured)
}

/// Only forge's own callsites are under test; hyper/wiremock chatter is not this crate's contract.
pub(crate) fn forge_lines(lines: &[CapturedLine]) -> Vec<&CapturedLine> {
    lines
        .iter()
        .filter(|line| line.target.starts_with("forge_"))
        .collect()
}
