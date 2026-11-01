use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_overlay::OverlayKindRegistry;
use forge_storage::{DataProvider, OverlayId};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Shared, SystemWallClock};

use crate::{
    EventBus, FrozenRuntime, LatestValues, NullEventLogRepo, OverlayFrameSink, OverlayReceivers,
    RuntimeWiring, RuntimeWiringParts,
};

pub(crate) use forge_storage::test_support::MemoryGlobals as MapGlobals;
pub(crate) use forge_storage_sqlite::test_support::{Sandboxed, sandboxed_backend};

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
        paths: forge_paths::AppPaths::at(std::env::temp_dir().join("forge-tests")),
        config: crate::Config::default(),
        os: crate::sub_action_runners::OsPorts::unavailable(),
        date_names: crate::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
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

pub(crate) struct WatchedObs(pub(crate) tokio::sync::watch::Receiver<bool>);

impl forge_platform_core::StreamOutputSignal for WatchedObs {
    fn is_active(&self) -> bool {
        *self.0.borrow()
    }

    async fn changed(&mut self) -> Option<bool> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}
