use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_overlay::{
    DeliveryDisposition, GENERATOR_VERSION, MaterializeReport, OverlayInstance,
    OverlayKindRegistry, OverlayMedia, SampleContext, SampleTrigger, delivered_content,
    ensure_shared_directory, materialize_overlay, read_overlay_source, remove_overlay_directory,
    sample_content, sample_context, write_overlay_source,
};
use forge_platform_core::paths;
use forge_registry::{
    KindPlatformContract, SubActionRegistry, TriggerRegistry, declared_variables,
};
use forge_storage::{
    OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, StorageError,
    reserved_keys,
};
use forge_types::ArgStack;
use serde_json::json;

use crate::actions::ActionsService;
use crate::bus::EventBus;
use crate::overlay_lanes::OverlayLanes;
use crate::overlay_media::{OverlayMediaLibrary, unresolvable};

pub const OVERLAY_TEST_FIRE_KIND: &str = "overlay.test_fire";

/// Browser-facing document keys are camelCase, matching the push envelope's `timeStamp`.
const OVERLAY_ID_KEY: &str = "overlayId";

#[derive(Debug, thiserror::Error)]
pub enum OverlayServiceError {
    #[error("no overlay is stored as '{0}'")]
    Unknown(OverlayId),

    #[error("overlay '{id}' needs an overlay type this build does not carry: {kind_id}")]
    UnavailableKind { id: OverlayId, kind_id: String },

    #[error("overlay '{id}' of type {kind_id} fills its own content and cannot take a send")]
    ContentNotAuthorable { id: OverlayId, kind_id: String },

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Overlay(#[from] forge_overlay::OverlayError),

    #[error("overlay file work did not finish")]
    Interrupted,
}

/// Counts only connections whose receiver was still alive; a page that closed counts for nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayReceivers {
    pub sources: usize,
    pub preview_tabs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayDelivery {
    Delivered { sources: usize },
    OnlyPreview { tabs: usize },
    NoPage,
}

impl OverlayReceivers {
    pub fn outcome(self) -> OverlayDelivery {
        match (self.sources, self.preview_tabs) {
            (0, 0) => OverlayDelivery::NoPage,
            (0, tabs) => OverlayDelivery::OnlyPreview { tabs },
            (sources, _) => OverlayDelivery::Delivered { sources },
        }
    }
}

/// Addressed at one overlay identity and never at the bus, so nothing delivered here runs an action.
#[async_trait]
pub trait OverlayFrameSink: Send + Sync {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers;

    async fn deliver_reload(&self, identity: &OverlayId);

    /// Called when an overlay is disabled: existing connections for `identity` go blank and
    /// close, independent of anything retained-content bookkeeping does.
    async fn revoke(&self, identity: &OverlayId);

    /// Read-only, sends nothing. The default answers zero for every identity; a sink backed by
    /// a live registry overrides it to report real connections.
    async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        let _ = identity;
        OverlayReceivers::default()
    }
}

/// The server calls this when a page's credential validates, which is the only moment the
/// runtime can know a browser source is back and needs what it was last showing.
#[async_trait]
pub trait OverlayConnectListener: Send + Sync {
    async fn overlay_connected(&self, identity: &OverlayId);
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFire {
    pub content: OverlayConfig,
    pub delivery: OverlayDelivery,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaterializePass {
    pub materialized: usize,
    pub unavailable: usize,
    pub failed: usize,
}

struct OverlayService {
    repo: Arc<dyn OverlayRepo>,
    settings: Arc<dyn SettingsRepo>,
    kinds: Arc<OverlayKindRegistry>,
    bus: Arc<EventBus>,
    frames: Option<Arc<dyn OverlayFrameSink>>,
    media: Option<OverlayMediaLibrary>,
    wiring: Option<EventWiring>,
    lanes: Arc<OverlayLanes>,
}

#[derive(Clone)]
struct EventWiring {
    actions: Arc<ActionsService>,
    sub_actions: Arc<SubActionRegistry>,
    triggers: Arc<TriggerRegistry>,
}

#[derive(Clone)]
pub struct OverlayServiceHandle {
    inner: Arc<OverlayService>,
}

impl OverlayServiceHandle {
    pub fn new(
        repo: Arc<dyn OverlayRepo>,
        settings: Arc<dyn SettingsRepo>,
        kinds: Arc<OverlayKindRegistry>,
        bus: Arc<EventBus>,
        frames: Option<Arc<dyn OverlayFrameSink>>,
    ) -> Self {
        Self {
            inner: Arc::new(OverlayService {
                repo,
                settings,
                kinds,
                bus,
                frames,
                media: None,
                wiring: None,
                lanes: Arc::default(),
            }),
        }
    }

    pub fn with_media_library(self, library: OverlayMediaLibrary) -> Self {
        Self {
            inner: Arc::new(OverlayService {
                media: Some(library),
                ..self.parts()
            }),
        }
    }

    pub fn with_event_wiring(
        self,
        actions: Arc<ActionsService>,
        sub_actions: Arc<SubActionRegistry>,
        triggers: Arc<TriggerRegistry>,
    ) -> Self {
        Self {
            inner: Arc::new(OverlayService {
                wiring: Some(EventWiring {
                    actions,
                    sub_actions,
                    triggers,
                }),
                ..self.parts()
            }),
        }
    }

    fn parts(&self) -> OverlayService {
        OverlayService {
            repo: Arc::clone(&self.inner.repo),
            settings: Arc::clone(&self.inner.settings),
            kinds: Arc::clone(&self.inner.kinds),
            bus: Arc::clone(&self.inner.bus),
            frames: self.inner.frames.clone(),
            media: self.inner.media.clone(),
            wiring: self.inner.wiring.clone(),
            lanes: Arc::clone(&self.inner.lanes),
        }
    }

    /// Read per call, so an overlay root changed in settings takes effect without a restart.
    pub async fn root(&self) -> PathBuf {
        match self
            .inner
            .settings
            .get_string(reserved_keys::SERVER_OVERLAY_ROOT)
            .await
        {
            Ok(Some(root)) if !root.is_empty() => PathBuf::from(root),
            Ok(_) => paths::overlays_dir(),
            Err(error) => {
                tracing::warn!(%error, "overlay root setting unreadable; using the default");
                paths::overlays_dir()
            }
        }
    }

    /// Rebuilds the whole root: an unknown overlay type keeps its record untouched, everything
    /// else is regenerated so a deleted root and a stale generator both recover on boot.
    pub async fn materialize_all(&self) -> Result<MaterializePass, OverlayServiceError> {
        let root = self.root().await;
        blocking(move || ensure_shared_directory(&root)).await??;

        let definitions = self.inner.repo.list().await?;
        let mut pass = MaterializePass::default();

        for definition in definitions {
            if self.inner.kinds.get(&definition.kind_id).is_none() {
                tracing::info!(
                    overlay = %definition.id,
                    kind_id = %definition.kind_id,
                    "overlay type is not in this build; record kept, page not regenerated"
                );
                pass.unavailable += 1;
                continue;
            }
            match self.write_files(&definition).await {
                Ok(_) => pass.materialized += 1,
                Err(error) => {
                    tracing::error!(overlay = %definition.id, %error, "overlay materialization failed");
                    pass.failed += 1;
                }
            }
        }

        Ok(pass)
    }

    /// Create, rename and config saves all land here; the directory name is the identity and never moves.
    pub async fn materialize(
        &self,
        id: &OverlayId,
    ) -> Result<MaterializeReport, OverlayServiceError> {
        let definition = self.load(id).await?;
        if self.inner.kinds.get(&definition.kind_id).is_none() {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        }

        let report = self.write_files(&definition).await?;
        self.reload_page(id).await;
        Ok(report)
    }

    /// Accepts only a name from `OVERRIDABLE_FILES`; `Ok(None)` while that file is not on disk yet.
    pub async fn read_source(
        &self,
        id: &OverlayId,
        file: &str,
    ) -> Result<Option<String>, OverlayServiceError> {
        let root = self.root().await;
        let identity = id.as_str().to_owned();
        let name = file.to_owned();
        Ok(blocking(move || read_overlay_source(&root, &identity, &name)).await??)
    }

    /// Accepts only a name from `OVERRIDABLE_FILES`; the record's override list is the caller's to
    /// keep in step, and nothing reloads until the caller says so.
    pub async fn write_source(
        &self,
        id: &OverlayId,
        file: &str,
        body: String,
    ) -> Result<(), OverlayServiceError> {
        let root = self.root().await;
        let identity = id.as_str().to_owned();
        let name = file.to_owned();
        Ok(blocking(move || write_overlay_source(&root, &identity, &name, &body)).await??)
    }

    pub async fn release_media(&self, id: &OverlayId) -> Result<u64, OverlayServiceError> {
        let Some(library) = &self.inner.media else {
            return Ok(0);
        };
        Ok(library.release(id).await?)
    }

    /// `Ok(false)` when the directory was already gone.
    pub async fn remove_folder(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let root = self.root().await;
        let identity = id.as_str().to_owned();
        Ok(blocking(move || remove_overlay_directory(&root, &identity)).await??)
    }

    /// Revokes the deleted identity's live connections, so a page still open under it never
    /// receives frames meant for a later overlay that reuses the identity.
    pub async fn delete(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let removed = self.inner.repo.delete(id).await?;
        if removed && let Some(frames) = &self.inner.frames {
            frames.revoke(id).await;
        }
        Ok(removed)
    }

    /// Persists first, then revokes any live connections when the overlay was just disabled.
    /// Retained content keeps recording either way - only delivery to a browser source stops.
    pub async fn set_enabled(
        &self,
        id: &OverlayId,
        enabled: bool,
    ) -> Result<bool, OverlayServiceError> {
        let changed = self.inner.repo.set_enabled(id, enabled).await?;
        if changed
            && !enabled
            && let Some(frames) = &self.inner.frames
        {
            frames.revoke(id).await;
        }
        Ok(changed)
    }

    /// Addressed at the pages carrying this identity; every other connection is untouched.
    pub async fn reload_page(&self, id: &OverlayId) {
        if let Some(frames) = &self.inner.frames {
            frames.deliver_reload(id).await;
        }
    }

    pub async fn receivers(&self, id: &OverlayId) -> OverlayReceivers {
        match &self.inner.frames {
            Some(frames) => frames.receivers(id).await,
            None => OverlayReceivers::default(),
        }
    }

    /// Replace content is persisted before it is sent, so a page that reconnects is handed the
    /// same values it was showing.
    pub async fn deliver_content(
        &self,
        id: &OverlayId,
        content: OverlayConfig,
        duration_ms: Option<u64>,
    ) -> Result<OverlayDelivery, OverlayServiceError> {
        let definition = self.load(id).await?;
        let disposition = self.disposition_of(&definition)?;
        self.push(&definition.id, disposition, &content, duration_ms)
            .await
    }

    /// The send-to-overlay step's funnel: the supplied fields are laid over the overlay's own
    /// content, both expanded against the run's arguments.
    pub async fn send_to(
        &self,
        id: &OverlayId,
        supplied: &OverlayConfig,
        args: &ArgStack,
        duration_ms: Option<u64>,
    ) -> Result<OverlayDelivery, OverlayServiceError> {
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        };
        if descriptor.content_is_machine_filled() {
            return Err(OverlayServiceError::ContentNotAuthorable {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        }

        let content = delivered_content(descriptor, &definition.config, supplied, args);
        let disposition = descriptor.delivery_disposition();
        self.push(&definition.id, disposition, &content, duration_ms)
            .await
    }

    /// Builds the sample content once and returns it, so the caller previews exactly what the
    /// page received. Nothing is published: no action, script or queue observes a test.
    pub async fn test_fire(&self, id: &OverlayId) -> Result<TestFire, OverlayServiceError> {
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        };

        let content = sample_content(
            descriptor,
            &definition.config,
            &self.sample_pass(&definition.id).await,
        );
        let disposition = descriptor.delivery_disposition();
        self.inner.bus.record(Event::new(
            EventSource::Core,
            OVERLAY_TEST_FIRE_KIND,
            json!({ OVERLAY_ID_KEY: definition.id.as_str() }),
        ));

        let delivery = self
            .push(&definition.id, disposition, &content, None)
            .await?;

        Ok(TestFire { content, delivery })
    }

    /// Only a Replace kind ever has a retained row, so what is stored is what may be replayed.
    async fn replay_retained(&self, id: &OverlayId) -> Result<(), OverlayServiceError> {
        let _lane = self.inner.lanes.enter(id).await;
        let Some(content) = self.inner.repo.get_retained_content(id).await? else {
            return Ok(());
        };
        self.send(id, &content, None).await;
        Ok(())
    }

    async fn push(
        &self,
        id: &OverlayId,
        disposition: DeliveryDisposition,
        content: &OverlayConfig,
        duration_ms: Option<u64>,
    ) -> Result<OverlayDelivery, OverlayServiceError> {
        if !disposition.retains_last_content() {
            return Ok(self.send(id, content, duration_ms).await);
        }
        let _lane = self.inner.lanes.enter(id).await;
        self.inner.repo.set_retained_content(id, content).await?;
        Ok(self.send(id, content, duration_ms).await)
    }

    async fn send(
        &self,
        id: &OverlayId,
        content: &OverlayConfig,
        duration_ms: Option<u64>,
    ) -> OverlayDelivery {
        let Some(frames) = &self.inner.frames else {
            return OverlayDelivery::NoPage;
        };
        frames
            .deliver_content(id, content_json(content), duration_ms)
            .await
            .outcome()
    }

    fn disposition_of(
        &self,
        definition: &OverlayDefinition,
    ) -> Result<DeliveryDisposition, OverlayServiceError> {
        self.inner
            .kinds
            .get(&definition.kind_id)
            .map(|descriptor| descriptor.delivery_disposition())
            .ok_or_else(|| OverlayServiceError::UnavailableKind {
                id: definition.id.clone(),
                kind_id: definition.kind_id.clone(),
            })
    }

    async fn load(&self, id: &OverlayId) -> Result<OverlayDefinition, OverlayServiceError> {
        self.inner
            .repo
            .get(id)
            .await?
            .ok_or_else(|| OverlayServiceError::Unknown(id.clone()))
    }

    async fn write_files(
        &self,
        definition: &OverlayDefinition,
    ) -> Result<MaterializeReport, OverlayServiceError> {
        let root = self.root().await;
        let instance = instance_of(
            definition,
            self.media_pass(definition).await,
            self.sample_pass(&definition.id).await,
        );
        let kinds = Arc::clone(&self.inner.kinds);
        let report = blocking(move || materialize_overlay(&root, &instance, &kinds)).await??;

        if !report.media_sweep_failures.is_empty() {
            tracing::warn!(
                overlay = %definition.id,
                paths = ?report.media_sweep_failures,
                "generated overlay media could not be swept"
            );
        }
        if !report.media_issues.is_empty() {
            tracing::warn!(
                overlay = %definition.id,
                issues = ?report.media_issues,
                "overlay media reference did not resolve; the page receives no file for it"
            );
        }

        if definition.generator_version != GENERATOR_VERSION {
            let mut stamped = definition.clone();
            stamped.generator_version = GENERATOR_VERSION;
            self.inner.repo.save(&stamped).await?;
        }

        Ok(report)
    }

    async fn sample_pass(&self, id: &OverlayId) -> SampleContext {
        let Some(wiring) = &self.inner.wiring else {
            return SampleContext::neutral();
        };
        let feeds = match wiring.actions.overlay_feeds(id, &wiring.sub_actions).await {
            Ok(feeds) => feeds,
            Err(error) => {
                tracing::warn!(overlay = %id, %error, "what feeds this overlay is unreadable; its sample stays neutral");
                return SampleContext::neutral();
            }
        };

        let feeding: Vec<SampleTrigger> = feeds
            .iter()
            .flat_map(|feed| feed.triggers.iter())
            .map(|instance| feeding_trigger(&wiring.triggers, &instance.kind_id))
            .collect();
        sample_context(&feeding)
    }

    async fn media_pass(&self, definition: &OverlayDefinition) -> OverlayMedia {
        let config = match self
            .inner
            .kinds
            .effective_config(&definition.kind_id, &definition.config)
        {
            Ok(config) => config,
            Err(_) => definition.config.clone(),
        };

        let Some(library) = &self.inner.media else {
            return unresolvable(&config).media;
        };

        let pass = library.resolve(&config).await;
        library.record(&definition.id, &pass).await;
        pass.media
    }
}

/// The service is built after the server, which is built after the sub-action registry, so a
/// runner registered at boot receives this and reads the handle once it exists.
#[derive(Clone, Default)]
pub struct OverlayServiceCell {
    inner: Arc<ArcSwapOption<OverlayServiceHandle>>,
}

impl OverlayServiceCell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, handle: OverlayServiceHandle) {
        self.inner.store(Some(Arc::new(handle)));
    }

    pub fn get(&self) -> Option<OverlayServiceHandle> {
        self.inner.load_full().map(|handle| (*handle).clone())
    }
}

#[async_trait]
impl OverlayConnectListener for OverlayServiceHandle {
    async fn overlay_connected(&self, identity: &OverlayId) {
        if let Err(error) = self.replay_retained(identity).await {
            tracing::warn!(overlay = %identity, %error, "retained overlay content did not replay");
        }
    }
}

fn content_json(content: &OverlayConfig) -> serde_json::Value {
    serde_json::Value::Object(
        content
            .iter()
            .map(|(key, value)| (key.clone(), value.to_plain_json()))
            .collect(),
    )
}

fn feeding_trigger(triggers: &TriggerRegistry, kind_id: &str) -> SampleTrigger {
    let descriptor = triggers.get(kind_id);
    SampleTrigger {
        kind_id: kind_id.to_owned(),
        contract: descriptor.map_or(KindPlatformContract::Universal, |held| {
            held.platform_contract()
        }),
        variables: descriptor.and_then(declared_variables).unwrap_or_default(),
    }
}

fn instance_of(
    definition: &OverlayDefinition,
    media: OverlayMedia,
    sample: SampleContext,
) -> OverlayInstance {
    OverlayInstance {
        id: definition.id.as_str().to_owned(),
        display_name: definition.display_name.clone(),
        kind_id: definition.kind_id.clone(),
        config: definition.config.clone(),
        source_overrides: definition.source_overrides.clone(),
        credential: Some(definition.credential.as_str().to_owned()),
        media,
        sample,
    }
}

/// Overlay file work is sync std fs; it never runs on a runtime worker.
async fn blocking<T, F>(work: F) -> Result<T, OverlayServiceError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| OverlayServiceError::Interrupted)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use forge_storage::overlay::MockOverlayRepo;
    use forge_storage::settings::MockSettingsRepo;

    use super::*;
    use crate::bus::NullEventLogRepo;

    const STAGE: &str = "stage-audio";

    struct CountingSink {
        identity: OverlayId,
        connected: OverlayReceivers,
    }

    #[async_trait]
    impl OverlayFrameSink for CountingSink {
        async fn deliver_content(
            &self,
            _: &OverlayId,
            _: serde_json::Value,
            _: Option<u64>,
        ) -> OverlayReceivers {
            unreachable!("counting the connected pages must never push a frame")
        }

        async fn deliver_reload(&self, _: &OverlayId) {
            unreachable!("counting the connected pages must never reload one")
        }

        async fn revoke(&self, _: &OverlayId) {
            unreachable!("counting the connected pages must never close one")
        }

        async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
            if identity == &self.identity {
                self.connected
            } else {
                OverlayReceivers::default()
            }
        }
    }

    /// Carries only what the trait demands, so its count comes from the default body.
    struct SilentSink;

    #[async_trait]
    impl OverlayFrameSink for SilentSink {
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

    fn handle_with(frames: Option<Arc<dyn OverlayFrameSink>>) -> OverlayServiceHandle {
        OverlayServiceHandle::new(
            Arc::new(MockOverlayRepo::new()),
            Arc::new(MockSettingsRepo::new()),
            Arc::new(OverlayKindRegistry::new()),
            EventBus::new(Arc::new(NullEventLogRepo)),
            frames,
        )
    }

    #[tokio::test]
    async fn the_connected_page_count_comes_from_the_sink_for_the_identity_asked() {
        let connected = OverlayReceivers {
            sources: 2,
            preview_tabs: 1,
        };
        let handle = handle_with(Some(Arc::new(CountingSink {
            identity: OverlayId::new(STAGE),
            connected,
        })));

        assert_eq!(handle.receivers(&OverlayId::new(STAGE)).await, connected);
        assert_eq!(
            handle.receivers(&OverlayId::new("alert-box")).await,
            OverlayReceivers::default(),
            "the handle asked the sink about an identity the caller never named"
        );
    }

    #[tokio::test]
    async fn no_pages_are_reported_when_nothing_is_counting_them() {
        for (label, handle) in [
            ("a runtime with no frame sink", handle_with(None)),
            (
                "a sink that does not count its connections",
                handle_with(Some(Arc::new(SilentSink))),
            ),
        ] {
            assert_eq!(
                handle.receivers(&OverlayId::new(STAGE)).await,
                OverlayReceivers::default(),
                "{label} reported pages that cannot exist"
            );
        }
    }

    #[derive(Default)]
    struct RevokeLog(std::sync::Mutex<Vec<OverlayId>>);

    impl RevokeLog {
        fn revoked(&self) -> Vec<OverlayId> {
            self.0.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl OverlayFrameSink for RevokeLog {
        async fn deliver_content(
            &self,
            _: &OverlayId,
            _: serde_json::Value,
            _: Option<u64>,
        ) -> OverlayReceivers {
            unreachable!("deleting an overlay must never push content")
        }

        async fn deliver_reload(&self, _: &OverlayId) {
            unreachable!("deleting an overlay must never reload a page")
        }

        async fn revoke(&self, identity: &OverlayId) {
            self.0.lock().unwrap().push(identity.clone());
        }
    }

    fn handle_deleting(
        outcome: fn() -> Result<bool, forge_storage::StorageError>,
        log: &Arc<RevokeLog>,
    ) -> OverlayServiceHandle {
        let mut repo = MockOverlayRepo::new();
        repo.expect_delete().times(1).returning(move |_| outcome());
        OverlayServiceHandle::new(
            Arc::new(repo),
            Arc::new(MockSettingsRepo::new()),
            Arc::new(OverlayKindRegistry::new()),
            EventBus::new(Arc::new(NullEventLogRepo)),
            Some(Arc::clone(log) as Arc<dyn OverlayFrameSink>),
        )
    }

    #[tokio::test]
    async fn deleting_an_overlay_revokes_its_live_pages_exactly_once() {
        let log = Arc::new(RevokeLog::default());
        let handle = handle_deleting(|| Ok(true), &log);

        assert!(handle.delete(&OverlayId::new(STAGE)).await.unwrap());

        assert_eq!(log.revoked(), [OverlayId::new(STAGE)]);
    }

    #[tokio::test]
    async fn a_delete_that_removes_no_row_revokes_nothing() {
        for (case, outcome) in [
            (
                "an identity the store does not hold",
                (|| Ok(false)) as fn() -> Result<bool, forge_storage::StorageError>,
            ),
            ("a store that failed the delete", || {
                Err(forge_storage::StorageError::Connection {
                    reason: "overlay store offline".to_owned(),
                })
            }),
        ] {
            let log = Arc::new(RevokeLog::default());
            let handle = handle_deleting(outcome, &log);

            let result = handle.delete(&OverlayId::new(STAGE)).await;

            assert!(!matches!(result, Ok(true)), "{case} reported a removal");
            assert!(log.revoked().is_empty(), "{case} closed live pages");
        }
    }
}
