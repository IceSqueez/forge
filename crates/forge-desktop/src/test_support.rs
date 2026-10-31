mod config_references;
mod memory_scripts;
pub(crate) mod pack_flow;
mod pack_transfer;
mod setup_transfer;

use forge_events::Event;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_overlay::SampleContext;
use forge_registry::ActorRole;
use forge_registry::{
    ActorBlock, ActorIdentity, ChatTriggerFamily, EventFilter, FormField, KindPlatformContract,
    LoginSlot, SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
    TriggerVariables,
};
use forge_runtime::{
    ActionCancelRegistry, EventBus, OverlayFrameSink, OverlayReceivers, OverlaySampleSource,
    spawn_action_engine,
};
use forge_speech_types::PcmBuffer;
use forge_storage::{DataProvider, OverlayId, SettingsRepo, StorageError};
use forge_types::{Action, ActionId, EventId, PlatformId, TriggerConfig};

pub(crate) use forge_host::test_support::{
    GatedSchedule, ReadFault, RecordingSink, SETTINGS_READ_FAILED, SETTINGS_WRITE_FAILED,
    SettingWrite, SilentPublisher, StubActions, StubEventLog, StubHistory, StubOverlays,
    TestBackend, alive_tasks, default_twitch_bot, every_registry_entry, gated_running_schedule,
    gated_running_schedule_sharing, install_tracing_interest_floor, keyless_enabled_schedule,
    neutral_pipeline_config, overlay_definition, overlay_named, pump, pump_until, runtime,
    stopped_handle, stopped_server_handle, stopped_server_hosting, stub_catalog, test_backend,
    test_backend_with_actions_and_triggers, test_backend_with_gated_export,
    test_backend_with_late_export, test_backend_with_media, test_backend_with_scripts,
    test_backend_with_scripts_and_actions, test_backend_with_stalling_export, within_hang_guard,
    writes_to, yield_until,
};
pub(crate) use memory_scripts::MemoryScripts;

pub(crate) async fn booted_runtime_handles() -> crate::runtime_handles::RuntimeHandles {
    let backend = forge_host::test_support::open_store().await;
    forge_host::test_support::seed_integrations_and_server_off(backend.as_ref()).await;
    forge_storage::write_setting(backend.as_ref(), &crate::stay_awake::STAY_AWAKE, &false)
        .await
        .unwrap();
    let pre_boot = crate::boot::run_pre_boot(&backend).await;
    let booted = forge_host::test_support::boot_host(
        Arc::clone(&backend),
        forge_host::test_support::fake_identities(),
        pre_boot.anonymous_donor.clone(),
    )
    .await;
    crate::boot::present_host(
        booted.host,
        backend,
        crate::log_tail::LogTail::new(),
        pre_boot,
    )
    .await
}

const PUMP_HANG_GUARD: std::time::Duration = forge_test_support::HANG_GUARD;
const WORKER_DRAIN_POLL: std::time::Duration = std::time::Duration::from_millis(1);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declares {
    Nothing,
    AnEmptyList,
    APrincipal,
}

pub(crate) struct StubTrigger {
    id: String,
    label: String,
    category: TriggerCategory,
    declares: Declares,
    fields: Vec<FormField>,
    defaults: TriggerConfig,
    chat_family: Option<ChatTriggerFamily>,
    contract: KindPlatformContract,
    filter_source: Option<forge_events::EventSource>,
    filter_prefix: Option<String>,
    refused: Option<(&'static str, &'static str)>,
}

impl StubTrigger {
    pub(crate) fn new(
        id: &str,
        label: &str,
        category: TriggerCategory,
        declares: Declares,
    ) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            category,
            declares,
            fields: Vec::new(),
            defaults: TriggerConfig::new(),
            chat_family: None,
            contract: KindPlatformContract::PlatformSpecific(PlatformId::Twitch),
            filter_source: None,
            filter_prefix: None,
            refused: None,
        }
    }

    pub(crate) fn refusing(mut self, key: &'static str, value: &'static str) -> Self {
        self.refused = Some((key, value));
        self
    }

    pub(crate) fn listening_to(
        mut self,
        source: Option<forge_events::EventSource>,
        prefix: &str,
    ) -> Self {
        self.filter_source = source;
        self.filter_prefix = Some(prefix.to_owned());
        self
    }

    pub(crate) fn with_contract(mut self, contract: KindPlatformContract) -> Self {
        self.contract = contract;
        self
    }

    pub(crate) fn with_fields(mut self, fields: Vec<FormField>) -> Self {
        self.fields = fields;
        self
    }

    pub(crate) fn with_defaults(mut self, defaults: TriggerConfig) -> Self {
        self.defaults = defaults;
        self
    }

    pub(crate) fn in_chat_family(mut self, family: ChatTriggerFamily) -> Self {
        self.chat_family = Some(family);
        self
    }
}

impl TriggerKindDescriptor for StubTrigger {
    fn id(&self) -> &str {
        &self.id
    }

    fn category(&self) -> TriggerCategory {
        self.category
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn summary(&self) -> &str {
        &self.label
    }

    fn search_text(&self) -> &str {
        &self.label
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        self.contract
    }

    fn default_config(&self) -> TriggerConfig {
        self.defaults.clone()
    }

    fn config_fields(&self) -> Vec<FormField> {
        self.fields.clone()
    }

    fn check_config(&self, config: &TriggerConfig) -> Result<(), forge_registry::ConfigProblem> {
        match self.refused {
            Some((key, value))
                if config.get(key).and_then(forge_types::Variant::as_str) == Some(value) =>
            {
                Err(forge_registry::ConfigProblem::field(
                    key,
                    forge_registry::ConfigReason::Required,
                ))
            }
            _ => Ok(()),
        }
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        self.chat_family
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: self.filter_source,
            kind_prefix: self.filter_prefix.clone(),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        match self.declares {
            Declares::Nothing => None,
            Declares::AnEmptyList => Some(TriggerVariables::new()),
            Declares::APrincipal => Some(TriggerVariables::new().actor(
                ActorBlock {
                    role: ActorRole::Principal,
                    platform: PlatformId::Twitch,
                    login: LoginSlot::Declared,
                },
                |_| ActorIdentity {
                    id: "1".to_owned(),
                    display_name: "Someone".to_owned(),
                    login: Some("someone".to_owned()),
                },
            )),
        }
    }
}

pub(crate) fn trigger_registry(stubs: Vec<StubTrigger>) -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    for stub in stubs {
        registry
            .register(Box::new(stub))
            .expect("every stub carries its own id");
    }
    registry
}

pub(crate) fn shown_in_english<F>(failure: F) -> String
where
    F: Into<crate::repo_failure::RepoFailure> + std::fmt::Display,
{
    crate::i18n::install_language(forge_storage::Language::En);
    let diagnostic = failure.to_string();
    let shown = failure.into().text();
    assert!(
        !shown.contains(&diagnostic),
        "the diagnostic reached the user: {shown}"
    );
    shown
}

pub(crate) fn worker_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("multi-thread runtime")
}

pub(crate) fn wait_until_worker_drains_to(rt: &tokio::runtime::Runtime, background_tasks: usize) {
    let deadline = std::time::Instant::now() + PUMP_HANG_GUARD;
    while alive_tasks(rt) > background_tasks {
        assert!(
            std::time::Instant::now() < deadline,
            "the worker runtime still runs {} tasks, {background_tasks} expected",
            alive_tasks(rt)
        );
        std::thread::sleep(WORKER_DRAIN_POLL);
    }
}

pub(crate) fn stored_shortcut_overrides(
    raw: Option<&str>,
) -> Result<crate::shortcut_overrides::ShortcutOverrides, StorageError> {
    let (backend, _writes) = test_backend();
    if let Some(raw) = raw {
        backend.seed_raw(&crate::shortcut_overrides::KEYBOARD_SHORTCUTS, raw);
    }
    runtime().block_on(crate::shortcut_overrides::load_overrides(backend))
}

pub(crate) const FORMAT_ISOLATES: [char; 2] = ['\u{2068}', '\u{2069}'];

pub(crate) fn settle_until(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    mut reached: impl FnMut(&mut gpui::TestAppContext) -> bool,
) {
    let deadline = std::time::Instant::now() + PUMP_HANG_GUARD;
    loop {
        cx.run_until_parked();
        if reached(cx) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the awaited state never arrived"
        );
        pump(rt);
    }
}

pub(crate) fn settle_until_tasks_drain_to(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    background_tasks: usize,
) {
    settle_until(cx, rt, |_| alive_tasks(rt) <= background_tasks);
    cx.run_until_parked();
}

pub(crate) fn serial_blocking_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("current-thread runtime")
}

pub(crate) fn settle_until_blocking_work_finishes(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
) {
    cx.run_until_parked();
    pump(rt);
    rt.block_on(within_hang_guard(async {
        tokio::task::spawn_blocking(|| ()).await.expect("fence");
    }));
    pump(rt);
    cx.run_until_parked();
}

pub(crate) struct StalledDownloader;

#[async_trait::async_trait]
impl crate::update_flow::ArtifactDownloader for StalledDownloader {
    async fn download(
        &self,
        _artifact: &forge_update::Artifact,
        _staging_dir: &std::path::Path,
    ) -> Result<forge_update::StagedArtifact, forge_update::UpdateError> {
        std::future::pending().await
    }
}

pub(crate) struct NeutralOverlaySamples;

#[async_trait::async_trait]
impl OverlaySampleSource for NeutralOverlaySamples {
    async fn sample(&self, _: &OverlayId) -> SampleContext {
        SampleContext::neutral()
    }
}

pub(crate) struct NoOverlayPages;

#[async_trait::async_trait]
impl OverlayFrameSink for NoOverlayPages {
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

pub(crate) use forge_storage_sqlite::test_support::{Sandboxed, sandboxed_backend_at};

pub(crate) fn sandboxed_over(provider: Arc<dyn DataProvider>) -> Sandboxed<Arc<dyn DataProvider>> {
    Sandboxed::new(
        provider,
        tempfile::tempdir().expect("a temporary media root"),
    )
}

pub(crate) fn held_queue_scheduler(
    rt: &tokio::runtime::Runtime,
    queue: forge_types::Queue,
) -> forge_runtime::QueueSchedulerHandle {
    rt.block_on(async {
        let bus = EventBus::new(Arc::new(StubEventLog));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            stub_catalog(),
            Arc::new(StubActions),
            Arc::new(StubHistory),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let id = queue.id;
        let scheduler = forge_runtime::QueueScheduler::spawn(engine, bus, vec![queue]);
        scheduler
            .set_mode(id, forge_runtime::QueueMode::HOLDING)
            .await
            .expect("the queue was registered at spawn");
        scheduler
    })
}

pub(crate) fn hold_one_dispatch(
    rt: &tokio::runtime::Runtime,
    scheduler: &forge_runtime::QueueSchedulerHandle,
    queue_id: forge_types::QueueId,
) {
    rt.block_on(scheduler.dispatch(forge_runtime::SchedulerRequest {
        queue_id,
        action_id: ActionId::new(),
        trigger_event_id: EventId::new(),
        trigger_kind: None,
        manual_test: false,
        initial_args: forge_types::ArgStack::new(),
        admission: forge_runtime::QueueAdmission::default(),
    }))
    .expect("the scheduler is running");
    pump(rt);
}

pub(crate) const REWARDS: &str = "rewards";

pub(crate) struct FakeCollections {
    listing:
        Mutex<forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>>,
    held_listings: Mutex<std::collections::VecDeque<HeldListing>>,
    released_listings: AtomicUsize,
    lists: AtomicUsize,
    signal: forge_platform_core::CollectionRevisionSignal,
}

type HeldListing = (
    tokio::sync::oneshot::Receiver<()>,
    forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
);

impl FakeCollections {
    pub(crate) fn listing(
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            listing: Mutex::new(listing),
            held_listings: Mutex::default(),
            released_listings: AtomicUsize::new(0),
            lists: AtomicUsize::new(0),
            signal: forge_platform_core::CollectionRevisionSignal::new(),
        })
    }

    pub(crate) fn relist(
        &self,
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) {
        *self.listing.lock().unwrap() = listing;
    }

    pub(crate) fn hold_next_listing(
        &self,
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) -> tokio::sync::oneshot::Sender<()> {
        let (release, released) = tokio::sync::oneshot::channel();
        self.held_listings
            .lock()
            .unwrap()
            .push_back((released, listing));
        release
    }

    pub(crate) fn bump(&self) {
        self.signal.bump();
    }

    pub(crate) fn list_calls(&self) -> usize {
        self.lists.load(Ordering::SeqCst)
    }

    pub(crate) fn released_listings(&self) -> usize {
        self.released_listings.load(Ordering::SeqCst)
    }

    pub(crate) fn metadata() -> forge_platform_core::CollectionMetadata {
        forge_platform_core::CollectionMetadata {
            id: forge_platform_core::CollectionId::new(REWARDS),
            label: forge_types::VerbatimText::new("Channel point rewards").into(),
            icon: forge_platform_core::SectionIcon::new("diamond"),
            capacity: None,
            fields: Vec::new(),
            toggles: Vec::new(),
        }
    }

    pub(crate) fn installed_as(
        self: &Arc<Self>,
        builtin: &str,
    ) -> forge_host::integrations::BuiltinRegistry {
        let registry = forge_host::integrations::BuiltinRegistry::default();
        registry.install(forge_host::integrations::BuiltinObject {
            collections: Some(Arc::clone(self) as Arc<dyn forge_platform_core::BuiltinCollections>),
            ..crate::unavailable_builtin::unavailable_builtin(&forge_types::IntegrationId::new(
                builtin,
            ))
        });
        registry
    }
}

pub(crate) fn reward(
    id: &str,
    title: &str,
    access: forge_platform_core::CollectionItemAccess,
) -> forge_platform_core::CollectionItem {
    forge_platform_core::CollectionItem {
        id: forge_platform_core::CollectionItemId::new(id),
        title: title.to_owned(),
        values: std::collections::BTreeMap::new(),
        toggles: std::collections::BTreeMap::new(),
        access,
    }
}

pub(crate) fn owned_reward(id: &str, title: &str) -> forge_platform_core::CollectionItem {
    reward(
        id,
        title,
        forge_platform_core::CollectionItemAccess::Manageable,
    )
}

pub(crate) fn foreign_reward(id: &str, title: &str) -> forge_platform_core::CollectionItem {
    reward(
        id,
        title,
        forge_platform_core::CollectionItemAccess::ReadOnly {
            reason: forge_types::VerbatimText::new("created elsewhere").into(),
        },
    )
}

#[async_trait::async_trait]
impl forge_platform_core::BuiltinCollections for FakeCollections {
    fn collections(&self) -> Vec<forge_platform_core::CollectionMetadata> {
        vec![Self::metadata()]
    }

    fn revisions(&self) -> forge_platform_core::CollectionRevisions {
        self.signal.subscribe()
    }

    async fn list(
        &self,
        _: &forge_platform_core::CollectionId,
    ) -> forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        let held = self.held_listings.lock().unwrap().pop_front();
        if let Some((released, listing)) = held {
            released.await.unwrap();
            self.released_listings.fetch_add(1, Ordering::SeqCst);
            return listing;
        }
        self.listing.lock().unwrap().clone()
    }

    async fn create(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn update(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn delete(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
    ) -> forge_platform_core::CollectionOutcome<()> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn set_toggle(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &str,
        _: bool,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }
}

pub(crate) const NESTING_KIND: &str = "stub.container";
pub(crate) const NESTED_CHAIN_KEY: &str = "body";

struct StubRunner {
    id: &'static str,
    nests: bool,
}

#[async_trait::async_trait]
impl forge_registry::SubActionRunner for StubRunner {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> forge_registry::SubActionCategory {
        forge_registry::SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn default_config(&self) -> forge_types::SubActionConfig {
        forge_types::SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        if self.nests {
            vec![FormField::SubChain {
                key: NESTED_CHAIN_KEY,
                label: "Body",
            }]
        } else {
            Vec::new()
        }
    }

    fn validate_config(
        &self,
        _: &forge_types::SubActionConfig,
    ) -> Result<(), forge_registry::RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _: &forge_types::SubActionConfig,
        _: &forge_registry::RunContext<'_>,
    ) -> (
        forge_types::SubActionTelemetry,
        Option<forge_types::ArgStack>,
    ) {
        unreachable!("stub steps are inspected, never run")
    }
}

pub(crate) fn owned_sub_actions(owned: &[(&'static str, &'static str)]) -> SubActionRegistry {
    let mut registry = SubActionRegistry::new();
    registry
        .register(Box::new(StubRunner {
            id: NESTING_KIND,
            nests: true,
        }))
        .expect("the container stub registers");
    for (kind, owner) in owned {
        registry
            .owned_by(forge_types::IntegrationId::from_static(owner))
            .register(Box::new(StubRunner {
                id: kind,
                nests: false,
            }))
            .expect("every owned stub carries its own id");
    }
    registry
}

pub(crate) fn owned_triggers(owned: &[(&'static str, &'static str)]) -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    for (kind, owner) in owned {
        registry
            .owned_by(forge_types::IntegrationId::from_static(owner))
            .register(Box::new(StubTrigger::new(
                kind,
                kind,
                TriggerCategory::Chat,
                Declares::Nothing,
            )))
            .expect("every owned stub carries its own id");
    }
    registry
}

pub(crate) fn step(kind_id: &str, enabled: bool) -> forge_types::SubActionStep {
    forge_types::SubActionStep {
        kind_id: kind_id.to_owned(),
        config: forge_types::SubActionConfig::new(),
        enabled,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub(crate) fn action_running(name: &str, steps: Vec<forge_types::SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: forge_types::QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
}

pub(crate) fn trigger_of(kind_id: &str) -> forge_types::TriggerInstance {
    forge_types::TriggerInstance {
        id: forge_types::TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: kind_id.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: forge_types::PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: forge_types::PermissionRung::Everyone,
    }
}

pub(crate) const STORAGE_KEY: [u8; 32] = [0x11; 32];

pub(crate) struct IdleFactory(pub(crate) forge_types::IntegrationId);

#[async_trait::async_trait]
impl forge_runtime::IntegrationFactory<forge_host::integrations::BuiltinObject> for IdleFactory {
    fn id(&self) -> forge_types::IntegrationId {
        self.0.clone()
    }

    fn teardown_grace(&self) -> std::time::Duration {
        forge_runtime::DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        Ok(true)
    }

    async fn start(
        &self,
    ) -> Result<
        forge_runtime::RunningIntegration<forge_host::integrations::BuiltinObject>,
        forge_runtime::IntegrationStartError,
    > {
        Ok(forge_runtime::RunningIntegration::idle())
    }
}

pub(crate) fn idle_supervisor(
    rt: &tokio::runtime::Runtime,
    settings: Arc<dyn SettingsRepo>,
    ids: &[forge_types::IntegrationId],
) -> forge_runtime::IntegrationSupervisor<forge_host::integrations::BuiltinObject> {
    let _entered = rt.enter();
    forge_runtime::IntegrationSupervisor::launch(
        ids.iter()
            .map(|id| {
                Arc::new(IdleFactory(id.clone()))
                    as Arc<
                        dyn forge_runtime::IntegrationFactory<
                                forge_host::integrations::BuiltinObject,
                            >,
                    >
            })
            .collect(),
        settings,
        forge_runtime::IntegrationGate::new(),
        Arc::new(forge_host::integrations::BuiltinRegistry::default()),
        forge_runtime::spawn_live_viewer_aggregator(&forge_net::TaskOwner::new()),
    )
}

pub(crate) async fn sandboxed_provider() -> Sandboxed<Arc<dyn DataProvider>> {
    sandboxed_backend_at("sqlite::memory:", STORAGE_KEY)
        .await
        .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
}

async fn default_queue(backend: &Arc<dyn DataProvider>) -> forge_types::QueueId {
    backend
        .queue_repo()
        .get(forge_types::DEFAULT_QUEUE_ID)
        .await
        .unwrap()
        .expect("migrations seed the default queue")
        .id
}

pub(crate) async fn seed_action(
    backend: &Arc<dyn DataProvider>,
    name: &str,
    steps: Vec<forge_types::SubActionStep>,
) -> ActionId {
    let action = Action {
        queue_id: default_queue(backend).await,
        ..action_running(name, steps)
    };
    backend.action_repo().save(&action).await.unwrap();
    action.id
}

pub(crate) async fn seed_trigger(
    backend: &Arc<dyn DataProvider>,
    kind_id: &str,
) -> forge_types::TriggerInstanceId {
    let instance = trigger_of(kind_id);
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .unwrap();
    instance.id
}

pub(crate) async fn link(
    backend: &Arc<dyn DataProvider>,
    action: ActionId,
    trigger: forge_types::TriggerInstanceId,
) {
    backend
        .trigger_instance_repo()
        .link_action(action, trigger)
        .await
        .unwrap();
}

pub(crate) fn lifecycle_switch(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    initial: forge_runtime::LifecycleStates,
) -> (
    gpui::Entity<crate::integration_lifecycle::IntegrationLifecycle>,
    crate::integration_switch::IntegrationSwitch,
) {
    let (settings, _writes) = test_backend();
    let supervisor = idle_supervisor(rt, settings as Arc<dyn SettingsRepo>, &[]);
    cx.update(|cx| {
        let lifecycle = gpui::AppContext::new(cx, |_| {
            crate::integration_lifecycle::IntegrationLifecycle::new(initial)
        });
        let switch =
            crate::integration_switch::IntegrationSwitch::new(lifecycle.clone(), supervisor);
        (lifecycle, switch)
    })
}

pub(crate) fn switch_lifecycle(
    cx: &mut gpui::TestAppContext,
    lifecycle: &gpui::Entity<crate::integration_lifecycle::IntegrationLifecycle>,
    states: forge_runtime::LifecycleStates,
) {
    lifecycle.update(cx, |lifecycle, cx| {
        lifecycle.replace(states);
        cx.notify();
    });
    cx.run_until_parked();
}

pub(crate) struct NavLog {
    screens: Vec<crate::navigation::Screen>,
    _sub: gpui::Subscription,
}

impl NavLog {
    pub(crate) fn screens(&self) -> Vec<crate::navigation::Screen> {
        self.screens.clone()
    }
}

pub(crate) fn nav_log<V: gpui::EventEmitter<crate::navigation::NavRequested>>(
    view: &gpui::Entity<V>,
    cx: &mut gpui::App,
) -> gpui::Entity<NavLog> {
    gpui::AppContext::new(cx, |cx| NavLog {
        screens: Vec::new(),
        _sub: cx.subscribe(
            view,
            |log: &mut NavLog, _, event: &crate::navigation::NavRequested, _| {
                log.screens.push(event.0.clone());
            },
        ),
    })
}

pub(crate) fn error_toasts(cx: &gpui::App) -> usize {
    cx.global::<forge_components::Toasts>()
        .items()
        .iter()
        .filter(|toast| toast.kind == forge_components::ToastKind::Error)
        .count()
}

struct BlankWindow;

impl gpui::Render for BlankWindow {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::Empty
    }
}

pub(crate) fn press_undo(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext as _;

    let action = cx.update(|cx| {
        let toasts = cx.global_mut::<forge_components::Toasts>();
        let id = toasts
            .items()
            .iter()
            .find(|toast| toast.kind == forge_components::ToastKind::Undo)
            .map(|toast| toast.id)
            .expect("an undo toast");
        toasts
            .take(id)
            .and_then(|toast| toast.action)
            .expect("an undo action")
    });
    let window: gpui::AnyWindowHandle = cx.add_window(|_, _| BlankWindow).into();
    cx.update(|cx| {
        cx.update_window(window, |_, window, cx| (action.on_action)(window, cx))
            .unwrap();
    });
}

pub(crate) fn blank_window(cx: &mut gpui::TestAppContext) {
    cx.add_window(|_, _| BlankWindow);
}

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(forge_components::Presentation::new(
            forge_components::ThemeId::ForgeDefault,
            forge_components::Density::Cozy,
        ))
    });
}

const OUTSIDE_TOOLBAR_HEIGHT: f32 = 40.0;
const FOCUS_HOPS_AFTER_A_CLICK: usize = 3;

pub(crate) struct ScreenUnderRoot<V> {
    root: gpui::FocusHandle,
    screen: gpui::Entity<V>,
    _forward: gpui::Subscription,
}

impl<V: gpui::Render> gpui::Render for ScreenUnderRoot<V> {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement, ParentElement, Styled, div, px};
        div()
            .track_focus(&self.root)
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_none().w_full().h(px(OUTSIDE_TOOLBAR_HEIGHT)))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.screen.clone()),
            )
    }
}

pub(crate) fn open_under_root<V: gpui::Render>(
    cx: &mut gpui::TestAppContext,
    screen: impl FnOnce(&mut gpui::App) -> gpui::Entity<V>,
) -> (gpui::Entity<V>, &mut gpui::VisualTestContext) {
    cx.update(crate::actions::register_key_bindings);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let root = cx.focus_handle();
        let forward = crate::navigable_list::forward_root_focus_to_list(&root, window, cx);
        ScreenUnderRoot {
            root,
            screen: screen(cx),
            _forward: forward,
        }
    });
    vcx.update(|window, _| window.activate_window());
    settle_focus(vcx);
    let screen = root.read_with(vcx, |root, _| root.screen.clone());
    (screen, vcx)
}

pub(crate) fn settle_focus(vcx: &mut gpui::VisualTestContext) {
    for _ in 0..FOCUS_HOPS_AFTER_A_CLICK {
        vcx.run_until_parked();
        vcx.update(|_, _| {});
    }
    vcx.run_until_parked();
}

pub(crate) fn click_outside_the_list(vcx: &mut gpui::VisualTestContext) {
    vcx.simulate_click(
        gpui::point(gpui::px(1.0), gpui::px(OUTSIDE_TOOLBAR_HEIGHT / 2.0)),
        gpui::Modifiers::none(),
    );
    settle_focus(vcx);
}

pub(crate) fn click_until(
    vcx: &mut gpui::VisualTestContext,
    area: &ClickArea,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) -> bool {
    let mut y = area.y.start + area.step_y / 2.0;
    while y < area.y.end {
        let mut x = area.x.start + area.step_x / 2.0;
        while x < area.x.end {
            vcx.simulate_click(
                gpui::point(gpui::px(x), gpui::px(y)),
                gpui::Modifiers::none(),
            );
            settle_focus(vcx);
            if done(vcx) {
                return true;
            }
            x += area.step_x;
        }
        y += area.step_y;
    }
    false
}

pub(crate) struct ClickArea {
    pub(crate) x: std::ops::Range<f32>,
    pub(crate) y: std::ops::Range<f32>,
    pub(crate) step_x: f32,
    pub(crate) step_y: f32,
}

pub(crate) fn click_grid(vcx: &mut gpui::VisualTestContext, area: ClickArea) {
    let mut y = area.y.start + area.step_y / 2.0;
    while y < area.y.end {
        let mut x = area.x.start + area.step_x / 2.0;
        while x < area.x.end {
            vcx.simulate_click(
                gpui::point(gpui::px(x), gpui::px(y)),
                gpui::Modifiers::none(),
            );
            x += area.step_x;
        }
        y += area.step_y;
    }
}

const CRUMB_WINDOW_W: f32 = 1000.0;
const CRUMB_WINDOW_H: f32 = 700.0;

pub(crate) fn click_selector(vcx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("nothing painted the selector {selector}"));
    vcx.simulate_click(bounds.center(), gpui::Modifiers::none());
}

pub(crate) fn hub_crumb_targets<V>(
    cx: &mut gpui::TestAppContext,
    category: Option<forge_platform_core::IntegrationCategory>,
    build: impl FnOnce(&mut gpui::Window, &mut gpui::Context<V>) -> V,
) -> Vec<crate::navigation::Screen>
where
    V: gpui::Render + gpui::EventEmitter<crate::navigation::NavRequested>,
{
    install_presentation(cx);
    let (view, vcx) = cx.add_window_view(build);
    let log = vcx.update(|_, cx| nav_log(&view, cx));
    vcx.simulate_resize(gpui::size(
        gpui::px(CRUMB_WINDOW_W),
        gpui::px(CRUMB_WINDOW_H),
    ));
    vcx.run_until_parked();
    click_selector(vcx, crate::hub_crumb::hub_crumb_id(category).leak());
    log.read_with(vcx, |log, _| log.screens())
}

pub(crate) fn quiet_bus(rt: &tokio::runtime::Runtime) -> Arc<EventBus> {
    rt.block_on(async { EventBus::new(Arc::new(StubEventLog)) })
}

pub(crate) type LiveAliases = forge_speak_queue::SpeakQueueHandle;

fn neutral_queue_deps(
    registry: forge_tts_core::TtsRegistry,
    aliases: Vec<forge_speech_types::VoiceAlias>,
) -> forge_speak_queue::QueueDeps {
    let resolver = forge_speak_queue::VoiceAliasResolver::new(
        aliases,
        forge_speech_types::AssignmentStrategy::DeterministicByName,
        forge_speech_types::IgnoreProfile::default(),
        forge_speech_types::SynthesisDefaults::default(),
    );
    forge_speak_queue::QueueDeps {
        registry,
        resolver,
        pipeline: forge_speak_queue::PipelineConfigHandle::new(neutral_pipeline_config()),
        audio_sink: Arc::new(forge_audio::NullSink),
        event_bus: Arc::new(SilentPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: HashMap::new(),
    }
}

pub(crate) fn spawn_speak_queue(
    aliases: Vec<forge_speech_types::VoiceAlias>,
) -> (
    forge_speak_queue::SpeakQueueHandle,
    forge_speak_queue::SpeakEventStream,
    LiveAliases,
) {
    let deps = neutral_queue_deps(forge_tts_core::TtsRegistry::new(), aliases);
    let (handle, stream) =
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
    (handle.clone(), stream, handle)
}

pub(crate) fn spawn_speak_queue_never_listing(
    engines: impl IntoIterator<Item = forge_speech_types::EngineId>,
) -> forge_speak_queue::SpeakQueueHandle {
    let mut deps = neutral_queue_deps(forge_tts_core::TtsRegistry::new(), Vec::new());
    deps.disabled_engines = engines.into_iter().collect();
    let (handle, _events) =
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
    handle
}

pub(crate) fn spawn_speak_queue_with_registry(
    registry: forge_tts_core::TtsRegistry,
) -> forge_speak_queue::SpeakQueueHandle {
    let deps = neutral_queue_deps(registry, Vec::new());
    let (handle, _events) =
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
    handle
}

struct ListedVoices {
    id: forge_speech_types::EngineId,
    voices: Vec<forge_speech_types::TtsVoice>,
}

#[async_trait::async_trait]
impl forge_tts_core::TtsEngine for ListedVoices {
    fn engine_id(&self) -> &forge_speech_types::EngineId {
        &self.id
    }

    fn capabilities(&self) -> &forge_tts_core::EngineCapabilities {
        static CAPABILITIES: forge_tts_core::EngineCapabilities =
            forge_tts_core::EngineCapabilities {
                ssml: false,
                neural_voices: false,
                streaming: false,
                custom_lexicons: false,
            };
        &CAPABILITIES
    }

    async fn list_voices(
        &self,
    ) -> Result<Vec<forge_speech_types::TtsVoice>, forge_tts_core::TtsError> {
        Ok(self.voices.clone())
    }

    async fn synthesize(
        &self,
        _request: forge_tts_core::SynthesisRequest,
    ) -> Result<PcmBuffer, forge_tts_core::TtsError> {
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct ListedVoicesFactory(
    forge_speech_types::EngineId,
    Vec<forge_speech_types::TtsVoice>,
);

impl forge_tts_core::TtsEngineFactory for ListedVoicesFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn forge_tts_core::TtsEngine>, forge_tts_core::TtsError> {
        Ok(Box::new(ListedVoices {
            id: self.0.clone(),
            voices: self.1.clone(),
        }))
    }
}

pub(crate) fn spawn_speak_queue_listing(
    rt: &tokio::runtime::Runtime,
    engine: &forge_speech_types::EngineId,
    voices: Vec<forge_speech_types::TtsVoice>,
) -> forge_speak_queue::SpeakQueueHandle {
    let expected = voices.len();
    let mut registry = forge_tts_core::TtsRegistry::new();
    registry.register(
        engine.clone(),
        Arc::new(ListedVoicesFactory(engine.clone(), voices)),
    );
    let deps = neutral_queue_deps(registry, Vec::new());
    let (handle, _events) = rt.block_on(async {
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps)
    });
    pump_until(rt, || handle.available_voices().len() == expected);
    handle
}

pub(crate) fn live_alias(
    resolver: &LiveAliases,
    viewer_id: &str,
) -> Option<forge_speech_types::VoiceAlias> {
    resolver
        .resolver_snapshot()
        .aliases()
        .iter()
        .find(|alias| alias.viewer_id == viewer_id)
        .cloned()
}

pub(crate) type AliasShape = (
    String,
    String,
    String,
    String,
    String,
    Option<f32>,
    Option<f32>,
    forge_speech_types::AliasState,
);

pub(crate) fn alias_shape(alias: &forge_speech_types::VoiceAlias) -> AliasShape {
    (
        alias.id.0.clone(),
        alias.viewer_id.clone(),
        alias.viewer_name.clone(),
        alias.engine_id.0.clone(),
        alias.voice_id.0.clone(),
        alias.pitch_semitones,
        alias.rate_multiplier,
        alias.state.clone(),
    )
}

struct CurrentSchemaDatabase;

impl forge_storage::DatabaseFileSchema for CurrentSchemaDatabase {
    fn applied_schema_version(&self, _: &std::path::Path) -> Result<Option<u32>, StorageError> {
        Ok(None)
    }

    fn inspect(
        &self,
        _: &std::path::Path,
    ) -> Result<Option<forge_storage::StoredDatabase>, StorageError> {
        Ok(Some(forge_storage::StoredDatabase {
            schema: forge_storage::StoredSchema::Current {
                version: forge_storage::EXPECTED_SCHEMA_VERSION,
            },
            holds_credentials: false,
        }))
    }
}

fn legacy_backup_bundle(dir: &std::path::Path) -> std::path::PathBuf {
    use std::io::Write;

    let database = b"restored database";
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(database.len() as u64);
    header.set_mode(0o600);
    builder
        .append_data(
            &mut header,
            format!(
                "{}/{}",
                forge_transfer::ARCHIVE_ROOT,
                forge_storage::DATABASE_FILE_NAME
            ),
            &database[..],
        )
        .unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().unwrap()).unwrap();
    let bundle = dir.join("legacy.tar.gz");
    std::fs::write(&bundle, gz.finish().unwrap()).unwrap();
    bundle
}

pub(crate) fn staged_restore_in(
    rt: &tokio::runtime::Runtime,
    data_dir: &std::path::Path,
) -> forge_transfer::StagedRestore {
    let bundles = tempfile::TempDir::new().unwrap();
    rt.block_on(forge_transfer::stage_full_restore(
        forge_transfer::StageRequest {
            bundle: legacy_backup_bundle(bundles.path()),
            data_dir: data_dir.to_path_buf(),
            passphrase: None,
            database_schema: Arc::new(CurrentSchemaDatabase),
        },
    ))
    .unwrap()
}

pub(crate) fn start_failure(detail: &str) -> forge_runtime::IntegrationStartError {
    forge_runtime::IntegrationStartError::new(
        forge_runtime::StartFailureCategory::Internal,
        forge_types::MessageRef::upstream_failure(forge_types::UpstreamDetail::from_redacted(
            detail,
        )),
    )
}
