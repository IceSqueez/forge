use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use tracing::field::{Field, Visit};
use tracing::span;
use tracing::subscriber::{DefaultGuard, Interest};
use tracing::{Level, Metadata, Subscriber};

const FORGE_TARGET_PREFIX: &str = "forge_";

#[derive(Clone, Debug)]
pub struct CapturedLine {
    pub target: String,
    pub level: Level,
    pub fields: BTreeMap<String, String>,
}

impl CapturedLine {
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    pub fn message(&self) -> &str {
        self.field("message").unwrap_or_default()
    }

    pub fn mentions(&self, needle: &str) -> bool {
        self.fields.values().any(|value| value.contains(needle))
    }

    pub fn field_names(&self) -> Vec<&str> {
        self.fields.keys().map(String::as_str).collect()
    }

    pub fn from_forge(&self) -> bool {
        self.target.starts_with(FORGE_TARGET_PREFIX)
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

struct InterestFloor;

impl Subscriber for InterestFloor {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, _: &Metadata<'_>) -> bool {
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
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        drop(tracing::subscriber::set_global_default(InterestFloor));
        tracing::callsite::rebuild_interest_cache();
    });
}

type SharedLines = Arc<Mutex<Vec<CapturedLine>>>;

fn locked(lines: &SharedLines) -> MutexGuard<'_, Vec<CapturedLine>> {
    lines.lock().unwrap_or_else(PoisonError::into_inner)
}

struct CaptureSubscriber {
    lines: SharedLines,
    max: Level,
}

impl Subscriber for CaptureSubscriber {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::sometimes()
    }

    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
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
        locked(&self.lines).push(CapturedLine {
            target: event.metadata().target().to_owned(),
            level: *event.metadata().level(),
            fields: collector.0,
        });
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

pub struct LogRecording {
    lines: SharedLines,
    _installed: DefaultGuard,
}

impl LogRecording {
    pub fn start(max: Level) -> Self {
        install_interest_floor();
        let lines = SharedLines::default();
        let installed = tracing::subscriber::set_default(CaptureSubscriber {
            lines: Arc::clone(&lines),
            max,
        });
        tracing::callsite::rebuild_interest_cache();
        Self {
            lines,
            _installed: installed,
        }
    }

    pub fn lines(&self) -> Vec<CapturedLine> {
        locked(&self.lines).clone()
    }
}

pub fn capture<T>(max: Level, body: impl FnOnce() -> T) -> (T, Vec<CapturedLine>) {
    let recording = LogRecording::start(max);
    let output = body();
    (output, recording.lines())
}

pub fn capture_blocking<F: Future>(max: Level, future: F) -> (F::Output, Vec<CapturedLine>) {
    capture(max, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime must build")
            .block_on(future)
    })
}

pub fn forge_lines(lines: &[CapturedLine]) -> Vec<&CapturedLine> {
    lines.iter().filter(|line| line.from_forge()).collect()
}

pub fn forge_warnings(lines: &[CapturedLine]) -> Vec<&CapturedLine> {
    lines
        .iter()
        .filter(|line| line.from_forge() && line.level == Level::WARN)
        .collect()
}

pub fn on_target<'a>(lines: &'a [CapturedLine], target: &str) -> Vec<&'a CapturedLine> {
    lines.iter().filter(|line| line.target == target).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_structured_fields_and_the_message_of_a_line() {
        let ((), lines) = capture(Level::INFO, || {
            tracing::info!(platform = "kick", attempt = 2, "reconnecting");
        });

        let line = &lines[0];
        assert_eq!(line.message(), "reconnecting");
        assert_eq!(line.field("platform"), Some("kick"));
        assert_eq!(line.field("attempt"), Some("2"));
    }

    #[test]
    fn drops_lines_finer_than_the_requested_level() {
        let ((), lines) = capture(Level::WARN, || {
            tracing::info!("too fine");
            tracing::warn!("kept");
        });

        let kept: Vec<&str> = lines.iter().map(CapturedLine::message).collect();
        assert_eq!(kept, ["kept"]);
    }

    #[test]
    fn a_recording_stops_at_its_drop() {
        let recording = LogRecording::start(Level::INFO);
        let shared = Arc::clone(&recording.lines);
        tracing::info!("during");
        drop(recording);

        tracing::info!("afterwards");

        assert_eq!(locked(&shared).len(), 1);
    }

    #[test]
    fn a_blocking_capture_sees_lines_from_the_future_it_drives() {
        let (answer, lines) = capture_blocking(Level::INFO, async {
            tokio::task::yield_now().await;
            tracing::info!("resumed");
            7
        });

        assert_eq!(answer, 7);
        assert_eq!(lines[0].message(), "resumed");
    }

    #[test]
    fn forge_warnings_ignore_foreign_targets_and_other_levels() {
        let ((), lines) = capture(Level::INFO, || {
            tracing::warn!(target: "hyper", "foreign");
            tracing::warn!(target: "forge_net", "ours");
            tracing::info!(target: "forge_net", "quiet");
        });

        let warnings: Vec<&str> = forge_warnings(&lines)
            .into_iter()
            .map(CapturedLine::message)
            .collect();
        assert_eq!(warnings, ["ours"]);
    }
}
