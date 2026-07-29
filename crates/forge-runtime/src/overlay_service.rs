use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_overlay::config::EVENT;
use forge_overlay::{
    GENERATOR_VERSION, MaterializeReport, OverlayInstance, OverlayKindRegistry,
    effective_overlay_config, ensure_shared_directory, materialize_overlay, read_overlay_source,
    remove_overlay_directory, sample_payload, write_overlay_source,
};
use forge_platform_core::paths;
use forge_storage::{
    OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, StorageError, reserved_keys,
};
use forge_types::Variant;
use serde_json::json;

use crate::bus::EventBus;

pub const OVERLAY_RELOAD_KIND: &str = "overlay.reload";
pub const OVERLAY_TEST_FIRE_KIND: &str = "overlay.test_fire";

/// Browser-facing document keys are camelCase, matching the push envelope's `timeStamp`.
const OVERLAY_ID_KEY: &str = "overlayId";
const EVENT_KIND_KEY: &str = "eventKind";

#[derive(Debug, thiserror::Error)]
pub enum OverlayServiceError {
    #[error("no overlay is stored as '{0}'")]
    Unknown(OverlayId),

    #[error("overlay '{id}' needs an overlay type this build does not carry: {kind_id}")]
    UnavailableKind { id: OverlayId, kind_id: String },

    #[error("overlay '{0}' has no event bound yet")]
    NoBoundEvent(OverlayId),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Overlay(#[from] forge_overlay::OverlayError),

    #[error("overlay file work did not finish")]
    Interrupted,
}

/// Reaches connected pages WITHOUT publishing to the bus, so a sample never runs an action.
#[async_trait]
pub trait OverlayFrameSink: Send + Sync {
    async fn deliver(&self, event: Event);
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestFire {
    pub event_kind: String,
    pub payload: serde_json::Value,
    /// False when nothing is serving, so the caller can say the preview ran alone.
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
        self.reload_page(id);
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

    /// Only pages carrying this identity reload; the control message is a bus event, not a sample.
    pub fn reload_page(&self, id: &OverlayId) {
        self.inner.bus.publish(Event::new(
            EventSource::Core,
            OVERLAY_RELOAD_KIND,
            json!({ OVERLAY_ID_KEY: id.as_str() }),
        ));
    }

    /// Builds the sample once and returns it, so the caller drives its preview from the very
    /// payload the page received. Nothing is published: no action, script or queue observes it.
    pub async fn test_fire(&self, id: &OverlayId) -> Result<TestFire, OverlayServiceError> {
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        };

        let config = effective_overlay_config(descriptor, &definition.config);
        let event_kind = config
            .get(EVENT)
            .and_then(Variant::as_str)
            .unwrap_or_default()
            .to_owned();
        if event_kind.is_empty() {
            return Err(OverlayServiceError::NoBoundEvent(definition.id));
        }

        let payload = sample_payload(&event_kind);
        let origin = Event::new(
            EventSource::Core,
            OVERLAY_TEST_FIRE_KIND,
            json!({ OVERLAY_ID_KEY: definition.id.as_str(), EVENT_KIND_KEY: &event_kind }),
        );
        let parent = origin.id;
        self.inner.bus.record(origin);

        let delivered = match &self.inner.frames {
            Some(frames) => {
                frames
                    .deliver(Event::caused_by(
                        source_of(&event_kind),
                        event_kind.clone(),
                        payload.clone(),
                        parent,
                    ))
                    .await;
                true
            }
            None => false,
        };

        Ok(TestFire {
            event_kind,
            payload,
            delivered,
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

fn instance_of(definition: &OverlayDefinition) -> OverlayInstance {
    OverlayInstance {
        id: definition.id.as_str().to_owned(),
        display_name: definition.display_name.clone(),
        kind_id: definition.kind_id.clone(),
        config: definition.config.clone(),
        source_overrides: definition.source_overrides.clone(),
    }
}

/// Kind grammar puts the source in the leading segment, so a sample frame carries the same
/// source a live frame of that kind would.
fn source_of(event_kind: &str) -> EventSource {
    match event_kind.split('.').next().unwrap_or_default() {
        "twitch" => EventSource::Twitch,
        "youtube" => EventSource::YouTube,
        "kick" => EventSource::Kick,
        "obs" => EventSource::Obs,
        "vtube" => EventSource::VTube,
        "discord" => EventSource::Discord,
        "midi" => EventSource::Midi,
        "hotkey" => EventSource::Hotkey,
        "timer" => EventSource::Timer,
        "http" => EventSource::Http,
        "script" => EventSource::Rhai,
        "ws" => EventSource::Server,
        "speak" | "audio" | "sound" => EventSource::Audio,
        _ => EventSource::Core,
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
