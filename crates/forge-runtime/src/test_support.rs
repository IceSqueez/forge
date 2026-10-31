use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_overlay::OverlayKindRegistry;
use forge_storage::{DataProvider, OverlayId};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Shared, SystemWallClock};
use tempfile::TempDir;

use crate::{
    EventBus, FrozenRuntime, LatestValues, NullEventLogRepo, OverlayFrameSink, OverlayReceivers,
    RuntimeWiring, RuntimeWiringParts,
};

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

struct NoPages;

#[async_trait]
impl OverlayFrameSink for NoPages {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        _: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        OverlayReceivers::default()
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

pub(crate) async fn frozen_core_runtime(
    key: [u8; 32],
) -> (FrozenRuntime, Sandboxed<Arc<SqliteBackend>>) {
    let backend = sandboxed_backend(key).await.map(Arc::new);
    let provider = Arc::clone(&*backend) as Arc<dyn DataProvider>;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let latest_values = LatestValues::load(
        provider.latest_value_repo(),
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        Shared::new(String::new()),
    )
    .await;
    let mut wiring = RuntimeWiring::new(RuntimeWiringParts {
        bus,
        provider,
        latest_values,
        script_speech: None,
        overlay_speech: None,
        overlay_kinds: Arc::new(OverlayKindRegistry::new()),
        overlay_frames: Arc::new(NoPages),
        clock: Arc::new(SystemWallClock),
    });
    wiring
        .register_core_sub_actions()
        .expect("the core steps register");
    wiring
        .register_scheduled_run_sub_actions()
        .expect("the scheduled-run steps register");
    (wiring.freeze().await, backend)
}

pub(crate) mod log_capture {

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
            drop(tracing::subscriber::set_global_default(InterestFloor));
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

struct ChannelViewerSource {
    reports: std::sync::Mutex<
        Option<tokio::sync::mpsc::UnboundedReceiver<forge_platform_core::ViewerReport>>,
    >,
}

impl forge_platform_core::LiveViewerSource for ChannelViewerSource {
    fn viewer_reports(&self) -> forge_platform_core::ViewerReportStream {
        let reports = self
            .reports
            .lock()
            .expect("mutex poisoned")
            .take()
            .expect("viewer_reports called once");
        Box::pin(tokio_stream::wrappers::UnboundedReceiverStream::new(
            reports,
        ))
    }
}

pub(crate) fn channel_viewer_source() -> (
    Box<dyn forge_platform_core::LiveViewerSource>,
    tokio::sync::mpsc::UnboundedSender<forge_platform_core::ViewerReport>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let source = Box::new(ChannelViewerSource {
        reports: std::sync::Mutex::new(Some(rx)),
    });
    (source, tx)
}

pub(crate) mod failure {
    use forge_types::{MessageArg, MessageRef, SubActionOutcome, UserText};

    pub(crate) fn message_of(text: &UserText) -> &MessageRef {
        match text {
            UserText::Message(message) => message,
            UserText::Verbatim(verbatim) => panic!("expected a coded message, got {verbatim:?}"),
        }
    }

    pub(crate) fn failure_message(outcome: &SubActionOutcome) -> &MessageRef {
        match outcome {
            SubActionOutcome::Failed(text) => message_of(text),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    pub(crate) fn verbatim_arg<'a>(message: &'a MessageRef, name: &str) -> Option<&'a str> {
        match message.arg(name) {
            Some(MessageArg::Verbatim(text)) => Some(text.as_str()),
            _ => None,
        }
    }

    pub(crate) fn detail_of(message: &MessageRef) -> Option<&str> {
        message.detail().map(|detail| detail.as_str())
    }

    pub(crate) fn rejected_field(text: &UserText) -> Option<&str> {
        let UserText::Message(message) = text else {
            return None;
        };
        match message.arg("field") {
            Some(MessageArg::StepFields { keys, .. }) if keys.len() == 1 => Some(keys[0].as_str()),
            _ => None,
        }
    }

    pub(crate) fn step_problem(kind: &str, problem: &forge_registry::ConfigProblem) -> UserText {
        problem
            .message(|keys| MessageArg::StepFields {
                kind: kind.to_owned(),
                keys: keys.to_vec(),
            })
            .into()
    }

    pub(crate) fn nested_problem(text: &UserText) -> Option<&MessageRef> {
        let UserText::Message(message) = text else {
            return None;
        };
        match message.arg("problem") {
            Some(MessageArg::Message(problem)) => Some(problem),
            _ => None,
        }
    }

    pub(crate) fn is_field_problem(text: &UserText, key: &str, code: &str) -> bool {
        rejected_field(text) == Some(key)
            && nested_problem(text).is_some_and(|problem| problem.code().to_string() == code)
    }

    pub(crate) fn plain_field_problem(kind: &str, key: &str, code: &str) -> UserText {
        let reason = forge_registry::ConfigReason::Coded(MessageRef::new(
            forge_types::MessageCode::parse(code).unwrap(),
        ));
        step_problem(kind, &forge_registry::ConfigProblem::field(key, reason))
    }
}
