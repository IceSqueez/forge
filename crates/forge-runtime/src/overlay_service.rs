use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_overlay::{
    DeliveryDisposition, GENERATOR_VERSION, MaterializeReport, OverlayInstance,
    OverlayKindRegistry, delivered_content, ensure_shared_directory, materialize_overlay,
    read_overlay_source, remove_overlay_directory, sample_content, write_overlay_source,
};
use forge_platform_core::paths;
use forge_storage::{
    OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, StorageError,
    reserved_keys,
};
use forge_types::ArgStack;
use serde_json::json;

use crate::bus::EventBus;

pub const OVERLAY_TEST_FIRE_KIND: &str = "overlay.test_fire";

/// Browser-facing document keys are camelCase, matching the push envelope's `timeStamp`.
const OVERLAY_ID_KEY: &str = "overlayId";

#[derive(Debug, thiserror::Error)]
pub enum OverlayServiceError {
    #[error("no overlay is stored as '{0}'")]
    Unknown(OverlayId),

    #[error("overlay '{id}' needs an overlay type this build does not carry: {kind_id}")]
    UnavailableKind { id: OverlayId, kind_id: String },

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Overlay(#[from] forge_overlay::OverlayError),

    #[error("overlay file work did not finish")]
    Interrupted,
}

/// Addressed at one overlay identity and never at the bus, so nothing delivered here runs an action.
#[async_trait]
pub trait OverlayFrameSink: Send + Sync {
    /// Returns how many connections for `identity` still had a live receiver when sent.
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> usize;

    async fn deliver_reload(&self, identity: &OverlayId);
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
    /// False when no connected overlay page received it, so the caller can say the preview ran alone.
    pub delivered: bool,
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
            }),
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

    /// `Ok(false)` when the directory was already gone.
    pub async fn remove_folder(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let root = self.root().await;
        let identity = id.as_str().to_owned();
        Ok(blocking(move || remove_overlay_directory(&root, &identity)).await??)
    }

    /// Addressed at the pages carrying this identity; every other connection is untouched.
    pub async fn reload_page(&self, id: &OverlayId) {
        if let Some(frames) = &self.inner.frames {
            frames.deliver_reload(id).await;
        }
    }

    /// `Ok(false)` when nothing is serving. Replace content is persisted before it is sent, so a
    /// page that reconnects is handed the same values it was showing.
    pub async fn deliver_content(
        &self,
        id: &OverlayId,
        content: OverlayConfig,
        duration_ms: Option<u64>,
    ) -> Result<bool, OverlayServiceError> {
        let definition = self.load(id).await?;
        let disposition = self.disposition_of(&definition)?;
        self.push(&definition.id, disposition, &content, duration_ms)
            .await
    }

    /// The show-overlay step's funnel: the supplied fields are laid over the overlay's own
    /// content, both expanded against the run's arguments. `Ok(false)` when nothing is serving.
    pub async fn show(
        &self,
        id: &OverlayId,
        supplied: &OverlayConfig,
        args: &ArgStack,
        duration_ms: Option<u64>,
    ) -> Result<bool, OverlayServiceError> {
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        };

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

        let content = sample_content(descriptor, &definition.config);
        let disposition = descriptor.delivery_disposition();
        self.inner.bus.record(Event::new(
            EventSource::Core,
            OVERLAY_TEST_FIRE_KIND,
            json!({ OVERLAY_ID_KEY: definition.id.as_str() }),
        ));

        let delivered = self
            .push(&definition.id, disposition, &content, None)
            .await?;

        Ok(TestFire { content, delivered })
    }

    /// Only a Replace kind ever has a retained row, so what is stored is what may be replayed.
    async fn replay_retained(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let Some(content) = self.inner.repo.get_retained_content(id).await? else {
            return Ok(false);
        };
        Ok(self.send(id, &content, None).await)
    }

    async fn push(
        &self,
        id: &OverlayId,
        disposition: DeliveryDisposition,
        content: &OverlayConfig,
        duration_ms: Option<u64>,
    ) -> Result<bool, OverlayServiceError> {
        if disposition.retains_last_content() {
            self.inner.repo.set_retained_content(id, content).await?;
        }
        Ok(self.send(id, content, duration_ms).await)
    }

    async fn send(
        &self,
        id: &OverlayId,
        content: &OverlayConfig,
        duration_ms: Option<u64>,
    ) -> bool {
        let Some(frames) = &self.inner.frames else {
            return false;
        };
        frames
            .deliver_content(id, content_json(content), duration_ms)
            .await
            > 0
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
        let instance = instance_of(definition);
        let kinds = Arc::clone(&self.inner.kinds);
        let report = blocking(move || materialize_overlay(&root, &instance, &kinds)).await??;

        if definition.generator_version != GENERATOR_VERSION {
            let mut stamped = definition.clone();
            stamped.generator_version = GENERATOR_VERSION;
            self.inner.repo.save(&stamped).await?;
        }

        Ok(report)
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
            .map(|(key, value)| (key.clone(), value.to_json()))
            .collect(),
    )
}

fn instance_of(definition: &OverlayDefinition) -> OverlayInstance {
    OverlayInstance {
        id: definition.id.as_str().to_owned(),
        display_name: definition.display_name.clone(),
        kind_id: definition.kind_id.clone(),
        config: definition.config.clone(),
        source_overrides: definition.source_overrides.clone(),
        credential: Some(definition.credential.as_str().to_owned()),
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
