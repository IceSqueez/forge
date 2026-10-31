#![allow(dead_code, clippy::expect_used, clippy::panic, clippy::unwrap_used)]

pub mod chain;
pub mod repos;
pub mod runners;
pub mod stub_provider;

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_overlay::{OverlayConfig, OverlayKindRegistry, SampleContext, register_builtin_kinds};
use forge_registry::SubActionRegistry;
use forge_runtime::{
    EventBus, FrozenRuntime, LatestValues, NullEventLogRepo, OverlayFrameSink, OverlayReceivers,
    OverlaySampleSource, RuntimeWiring, RuntimeWiringParts, SystemWallClock, WallClock,
};
use forge_storage::{DataProvider, OverlayDefinition, OverlayId};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ExecutionMode, MessageArg, MessageRef, OverlayCredential, QueueId, Shared,
    SubActionOutcome, SubActionStep, UserText, VerbatimText,
};
use stub_provider::StubProvider;
use tempfile::TempDir;
use time::OffsetDateTime;

pub const TEST_KEY: [u8; 32] = [0xcd; 32];
pub const BASE_UNIX_SECS: i64 = 1_791_115_200;
const PAGE_CREDENTIAL: &str = "2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d";
const IN_MEMORY_DATABASE: &str = ":memory:";
const MEDIA_DIR: &str = "media";

pub async fn sqlite_backend() -> (SqliteBackend, TempDir) {
    let media = tempfile::tempdir().unwrap();
    let backend = SqliteBackend::open_for_test(
        IN_MEMORY_DATABASE,
        TEST_KEY,
        media.path().join(MEDIA_DIR),
        None,
    )
    .await
    .unwrap();
    (backend, media)
}

pub fn sequential_action(
    id: ActionId,
    name: &str,
    queue_id: QueueId,
    sub_actions: Vec<SubActionStep>,
) -> Action {
    Action {
        id,
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions,
    }
}

pub fn overlay_definition(
    id: &str,
    display_name: &str,
    kind_id: &str,
    config: OverlayConfig,
) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: display_name.to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config,
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new(PAGE_CREDENTIAL),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

pub fn base() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(BASE_UNIX_SECS).unwrap()
}

pub struct FixedClock;

impl WallClock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        base()
    }
}

pub fn message_of(text: &UserText) -> &MessageRef {
    match text {
        UserText::Message(message) => message,
        UserText::Verbatim(verbatim) => panic!("expected a coded message, got {verbatim:?}"),
    }
}

pub fn failure_message(outcome: &SubActionOutcome) -> &MessageRef {
    match outcome {
        SubActionOutcome::Failed(text) => message_of(text),
        other => panic!("expected Failed, got {other:?}"),
    }
}

pub fn code_of(message: &MessageRef) -> String {
    message.code().to_string()
}

pub fn verbatim_arg<'a>(message: &'a MessageRef, name: &str) -> Option<&'a str> {
    match message.arg(name) {
        Some(MessageArg::Verbatim(text)) => Some(text.as_str()),
        _ => None,
    }
}

pub fn detail_of(message: &MessageRef) -> Option<&str> {
    message.detail().map(|detail| detail.as_str())
}

pub fn verbatim(text: &str) -> MessageArg {
    MessageArg::Verbatim(VerbatimText::new(text))
}

pub fn rejected_field(text: &UserText) -> Option<&str> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("field") {
        Some(MessageArg::StepFields { keys, .. }) if keys.len() == 1 => Some(keys[0].as_str()),
        _ => None,
    }
}

pub fn step_problem(kind: &str, problem: &forge_registry::ConfigProblem) -> UserText {
    problem
        .message(|keys| MessageArg::StepFields {
            kind: kind.to_owned(),
            keys: keys.to_vec(),
        })
        .into()
}

pub fn nested_problem(text: &UserText) -> Option<&MessageRef> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("problem") {
        Some(MessageArg::Message(problem)) => Some(problem),
        _ => None,
    }
}

pub fn is_field_problem(text: &UserText, key: &str, code: &str) -> bool {
    rejected_field(text) == Some(key)
        && nested_problem(text).is_some_and(|problem| problem.code().to_string() == code)
}

pub fn plain_field_problem(kind: &str, key: &str, code: &str) -> UserText {
    let reason = forge_registry::ConfigReason::Coded(MessageRef::new(
        forge_types::MessageCode::parse(code).unwrap(),
    ));
    step_problem(kind, &forge_registry::ConfigProblem::field(key, reason))
}

pub struct NeutralSamples;

#[async_trait]
impl OverlaySampleSource for NeutralSamples {
    async fn sample(&self, _: &OverlayId) -> SampleContext {
        SampleContext::neutral()
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

pub fn no_pages() -> Arc<dyn OverlayFrameSink> {
    Arc::new(NoPages)
}

pub fn builtin_overlay_kinds() -> Arc<OverlayKindRegistry> {
    let mut kinds = OverlayKindRegistry::new();
    register_builtin_kinds(&mut kinds).unwrap();
    Arc::new(kinds)
}

pub async fn wiring_parts(
    provider: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
) -> RuntimeWiringParts {
    let latest_values = LatestValues::load(
        provider.latest_value_repo(),
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        Shared::new(String::new()),
    )
    .await;
    RuntimeWiringParts {
        bus,
        provider,
        latest_values,
        script_speech: None,
        overlay_speech: None,
        overlay_kinds: builtin_overlay_kinds(),
        overlay_frames: no_pages(),
        clock: Arc::new(SystemWallClock),
    }
}

pub fn with_every_core_step(mut wiring: RuntimeWiring) -> RuntimeWiring {
    wiring.register_core_sub_actions().unwrap();
    wiring.register_scheduled_run_sub_actions().unwrap();
    wiring
}

pub async fn frozen_runtime(parts: RuntimeWiringParts) -> FrozenRuntime {
    with_every_core_step(RuntimeWiring::new(parts))
        .freeze()
        .await
}

pub async fn core_steps() -> Arc<SubActionRegistry> {
    let provider: Arc<dyn DataProvider> = Arc::new(StubProvider::idle());
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    frozen_runtime(wiring_parts(provider, bus).await)
        .await
        .sub_action_registry
}

pub fn core_steps_off_any_runtime() -> Arc<SubActionRegistry> {
    std::thread::spawn(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(core_steps())
    })
    .join()
    .unwrap()
}
