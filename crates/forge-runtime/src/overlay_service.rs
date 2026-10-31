use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_overlay::{
    DEFAULT_DISPLAY_SECS, DeliveryDisposition, GENERATOR_VERSION, MaterializeReport,
    OverlayInstance, OverlayKindDescriptor, OverlayKindRegistry, OverlayMedia, SampleContext,
    delivered_content, ensure_shared_directory, joined_to_show, latest_binding, latest_content,
    materialize_overlay, read_overlay_source, refresh_config_credential, remove_overlay_directory,
    sample_content, show_timing, take_speech, upgrade_config, write_overlay_source,
};
use forge_registry::CancelSignal;
use forge_storage::{
    OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo, StorageError,
    read_setting, settings::core_settings,
};
use forge_types::{ArgStack, EventId, LatestValueReader};
use serde_json::json;
use ulid::Ulid;

use crate::bus::EventBus;
use crate::overlay_definition_revision::{OverlayDefinitionChanges, OverlayDefinitionRevision};
use crate::overlay_lanes::OverlayLanes;
use crate::overlay_media::{OverlayMediaLibrary, unresolvable};
use crate::overlay_samples::OverlaySampleSource;
use crate::overlay_shows::{
    QueueFull, SHOW_CEILING, SHOW_QUEUE_CAPACITY, Show, ShowDepthWatch, ShowEnd, ShowSequencer,
    ShowTicket,
};
use crate::speak_dispatcher::{
    ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeechOrigin, SpeechStartSignal,
};

pub const OVERLAY_TEST_FIRE_KIND: &str = "overlay.test_fire";

pub const OVERLAY_SPEECH_FAILED_KIND: &str = "overlay.speech.failed";

pub const OVERLAY_NAME_KEY: &str = "overlayName";

pub const SPEECH_FAILURE_ERROR_KEY: &str = "error";

pub const SPEECH_FAILURE_MESSAGE_KEY: &str = "errorMessage";

const SPEECH_FAILURE_VOICE_KEY: &str = "voice";

const OVERLAY_ID_KEY: &str = "overlayId";

const LATEST_PREVIEW_HOLD: Duration = Duration::from_secs(DEFAULT_DISPLAY_SECS.unsigned_abs());

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

    #[error("overlay '{id}' already has {capacity} shows waiting, so this one was not queued")]
    ShowQueueFull { id: OverlayId, capacity: usize },

    #[error("overlay '{0}' shows a latest value and takes no content from actions")]
    SlotBound(OverlayId),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayReceivers {
    pub sources: usize,
    pub preview_tabs: usize,
}

#[derive(Debug)]
pub enum OverlayDispatch {
    Applied(OverlayDelivery),
    Queued(ShowTicket),
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

#[async_trait]
pub trait OverlayFrameSink: Send + Sync {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers;

    async fn deliver_reload(&self, identity: &OverlayId);

    async fn revoke(&self, identity: &OverlayId);

    async fn receivers(&self, identity: &OverlayId) -> OverlayReceivers {
        let _ = identity;
        OverlayReceivers::default()
    }
}

#[async_trait]
pub trait OverlayConnectListener: Send + Sync {
    async fn overlay_connected(&self, identity: &OverlayId);
}

pub struct OverlayConnectFanout(Vec<Arc<dyn OverlayConnectListener>>);

impl OverlayConnectFanout {
    pub fn new(listeners: Vec<Arc<dyn OverlayConnectListener>>) -> Self {
        Self(listeners)
    }
}

#[async_trait]
impl OverlayConnectListener for OverlayConnectFanout {
    async fn overlay_connected(&self, identity: &OverlayId) {
        for listener in &self.0 {
            listener.overlay_connected(identity).await;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnabledOverlay {
    pub id: OverlayId,
    pub latest_slot: Option<String>,
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
    frames: Arc<dyn OverlayFrameSink>,
    media: Option<OverlayMediaLibrary>,
    lanes: Arc<OverlayLanes>,
    shows: Arc<ShowSequencer>,
    speaker: Option<Arc<dyn SpeakDispatcher>>,
    latest: Option<Arc<dyn LatestValueReader>>,
    definitions: OverlayDefinitionRevision,
}

pub struct OverlayServiceParts {
    pub repo: Arc<dyn OverlayRepo>,
    pub settings: Arc<dyn SettingsRepo>,
    pub kinds: Arc<OverlayKindRegistry>,
    pub bus: Arc<EventBus>,
    pub frames: Arc<dyn OverlayFrameSink>,
    pub media: Option<OverlayMediaLibrary>,
    pub speaker: Option<Arc<dyn SpeakDispatcher>>,
    pub latest: Option<Arc<dyn LatestValueReader>>,
}

#[derive(Clone)]
pub struct OverlayServiceHandle {
    inner: Arc<OverlayService>,
}

impl OverlayServiceHandle {
    pub fn new(parts: OverlayServiceParts) -> Self {
        let OverlayServiceParts {
            repo,
            settings,
            kinds,
            bus,
            frames,
            media,
            speaker,
            latest,
        } = parts;
        let shows = Arc::new(ShowSequencer::new(Arc::clone(&frames), speaker.clone()));
        Self {
            inner: Arc::new(OverlayService {
                repo,
                settings,
                kinds,
                bus,
                frames,
                media,
                lanes: Arc::default(),
                shows,
                speaker,
                latest,
                definitions: OverlayDefinitionRevision::default(),
            }),
        }
    }

    pub fn definition_changes(&self) -> OverlayDefinitionChanges {
        self.inner.definitions.subscribe()
    }

    pub async fn enabled_overlays(&self) -> Result<Vec<EnabledOverlay>, OverlayServiceError> {
        let definitions = self.inner.repo.list().await?;
        Ok(definitions
            .into_iter()
            .filter(|definition| definition.enabled)
            .map(|definition| {
                let latest_slot = self
                    .inner
                    .kinds
                    .get(&definition.kind_id)
                    .and_then(|descriptor| latest_binding(descriptor, &definition.config))
                    .map(|binding| binding.slot);
                EnabledOverlay {
                    id: definition.id,
                    latest_slot,
                }
            })
            .collect())
    }

    pub async fn root(&self) -> PathBuf {
        match read_setting(
            self.inner.settings.as_ref(),
            &core_settings::SERVER_OVERLAY_ROOT,
        )
        .await
        {
            Ok(Some(root)) if !root.is_empty() => PathBuf::from(root),
            Ok(_) => forge_paths::overlays_dir(),
            Err(error) => {
                tracing::warn!(%error, "overlay root setting unreadable; using the default");
                forge_paths::overlays_dir()
            }
        }
    }

    pub async fn materialize_all(
        &self,
        samples: &dyn OverlaySampleSource,
    ) -> Result<MaterializePass, OverlayServiceError> {
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
                match self.refresh_credential(&definition).await {
                    Ok(_) => pass.unavailable += 1,
                    Err(error) => {
                        tracing::error!(overlay = %definition.id, %error, "credential in the kept overlay config could not be refreshed");
                        pass.failed += 1;
                    }
                }
                continue;
            }
            match self.write_files(&definition, samples).await {
                Ok(_) => pass.materialized += 1,
                Err(error) => {
                    tracing::error!(overlay = %definition.id, %error, "overlay materialization failed");
                    pass.failed += 1;
                }
            }
        }

        Ok(pass)
    }

    async fn refresh_credential(
        &self,
        definition: &OverlayDefinition,
    ) -> Result<(), OverlayServiceError> {
        let root = self.root().await;
        let identity = definition.id.as_str().to_owned();
        let credential = definition.credential.expose().to_owned();
        blocking(move || refresh_config_credential(&root, &identity, &credential)).await??;
        Ok(())
    }

    pub async fn materialize(
        &self,
        id: &OverlayId,
        samples: &dyn OverlaySampleSource,
    ) -> Result<MaterializeReport, OverlayServiceError> {
        let definition = self.load(id).await?;
        if self.inner.kinds.get(&definition.kind_id).is_none() {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        }

        let report = self.write_files(&definition, samples).await?;
        self.inner.definitions.advance();
        self.reload_page(id).await;
        Ok(report)
    }

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

    pub async fn remove_folder(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let root = self.root().await;
        let identity = id.as_str().to_owned();
        Ok(blocking(move || remove_overlay_directory(&root, &identity)).await??)
    }

    pub async fn delete(&self, id: &OverlayId) -> Result<bool, OverlayServiceError> {
        let removed = self.inner.repo.delete(id).await?;
        if removed {
            self.inner.definitions.advance();
            self.withdraw_shows(id);
            self.inner.frames.revoke(id).await;
        }
        Ok(removed)
    }

    pub async fn save(&self, definition: &OverlayDefinition) -> Result<(), OverlayServiceError> {
        self.inner.kinds.validate_stored(
            &definition.kind_id,
            definition.config_schema_version,
            &definition.config,
        )?;
        Ok(self.inner.repo.save(definition).await?)
    }

    pub async fn update_config(
        &self,
        id: &OverlayId,
        config: &OverlayConfig,
    ) -> Result<bool, OverlayServiceError> {
        let Some(definition) = self.inner.repo.get(id).await? else {
            return Ok(false);
        };
        self.inner.kinds.validate_stored(
            &definition.kind_id,
            definition.config_schema_version,
            config,
        )?;
        Ok(self.inner.repo.update_config(id, config).await?)
    }

    pub async fn rename(
        &self,
        id: &OverlayId,
        display_name: &str,
    ) -> Result<bool, OverlayServiceError> {
        Ok(self.inner.repo.rename(id, display_name).await?)
    }

    pub async fn set_enabled(
        &self,
        id: &OverlayId,
        enabled: bool,
    ) -> Result<bool, OverlayServiceError> {
        let changed = self.inner.repo.set_enabled(id, enabled).await?;
        if changed {
            self.inner.definitions.advance();
        }
        if changed && !enabled {
            self.withdraw_shows(id);
            self.inner.frames.revoke(id).await;
        }
        Ok(changed)
    }

    pub async fn reload_page(&self, id: &OverlayId) {
        self.inner.frames.deliver_reload(id).await;
    }

    pub async fn receivers(&self, id: &OverlayId) -> OverlayReceivers {
        self.inner.frames.receivers(id).await
    }

    pub async fn deliver_audio(
        &self,
        id: &OverlayId,
        content: OverlayConfig,
    ) -> Result<OverlayDelivery, OverlayServiceError> {
        let definition = self.load(id).await?;
        Ok(self.send(&definition.id, &content, None).await)
    }

    pub async fn send_to(
        &self,
        id: &OverlayId,
        supplied: &OverlayConfig,
        args: &ArgStack,
        duration_ms: Option<u64>,
        caused_by: Option<EventId>,
    ) -> Result<OverlayDispatch, OverlayServiceError> {
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Err(OverlayServiceError::UnavailableKind {
                id: definition.id,
                kind_id: definition.kind_id,
            });
        };
        if latest_binding(descriptor, &definition.config).is_some() {
            return Err(OverlayServiceError::SlotBound(definition.id));
        }
        let mut content = delivered_content(descriptor, &definition.config, supplied, args);
        let speech = take_speech(descriptor, &definition.config, &mut content)
            .filter(|_| self.inner.speaker.is_some())
            .map(|program| ShowSpeech {
                text: program.text,
                voice: program.voice,
                overlay: definition.id.as_str().to_owned(),
                show: Ulid::generate().to_string(),
                origin: SpeechOrigin::from_args(args, caused_by),
            });
        let speech =
            speech.and_then(|speech| self.with_installed_voice(&definition, speech, caused_by));
        let Some(timing) = show_timing(descriptor, &definition.config, duration_ms) else {
            let disposition = descriptor.delivery_disposition();
            let delivery = self
                .push(&definition.id, disposition, &content, duration_ms)
                .await?;
            if let Some(speech) = speech {
                self.speak_unheld(speech);
            }
            return Ok(OverlayDispatch::Applied(delivery));
        };

        if let Some(speech) = &speech {
            content = joined_to_show(content, &speech.show);
        }
        let ceiling_ms = u64::try_from(SHOW_CEILING.as_millis()).unwrap_or(u64::MAX);
        let show = Show {
            content: content_json(&content),
            duration_ms: duration_ms.map(|ms| ms.min(ceiling_ms)),
            window: timing.window.min(SHOW_CEILING),
            exit_tail: timing.exit_tail,
            speech,
        };
        self.inner
            .shows
            .enqueue(&definition.id, show)
            .map(OverlayDispatch::Queued)
            .map_err(|QueueFull| OverlayServiceError::ShowQueueFull {
                id: definition.id,
                capacity: SHOW_QUEUE_CAPACITY,
            })
    }

    fn with_installed_voice(
        &self,
        definition: &OverlayDefinition,
        speech: ShowSpeech,
        caused_by: Option<EventId>,
    ) -> Option<ShowSpeech> {
        let (Some(voice), Some(speaker)) = (speech.voice.as_deref(), self.inner.speaker.as_ref())
        else {
            return Some(speech);
        };
        let Err(error) = speaker.check_voice(voice) else {
            return Some(speech);
        };
        tracing::info!(
            overlay = %definition.id,
            reason = %error,
            "show speech skipped; the show is shown without it"
        );
        let failure = SpeakDispatchError::from(error);
        let payload = json!({
            OVERLAY_ID_KEY: definition.id.as_str(),
            OVERLAY_NAME_KEY: definition.display_name,
            SPEECH_FAILURE_VOICE_KEY: voice,
            SPEECH_FAILURE_ERROR_KEY: failure.message().code().to_string(),
            SPEECH_FAILURE_MESSAGE_KEY: failure.message(),
        });
        let event = match caused_by {
            Some(parent) => Event::caused_by(
                EventSource::Core,
                OVERLAY_SPEECH_FAILED_KIND,
                payload,
                parent,
            ),
            None => Event::new(EventSource::Core, OVERLAY_SPEECH_FAILED_KIND, payload),
        };
        self.inner.bus.publish(event);
        None
    }

    fn speak_unheld(&self, speech: ShowSpeech) {
        let Some(speaker) = self.inner.speaker.clone() else {
            return;
        };
        tokio::spawn(async move {
            let overlay = speech.overlay.clone();
            if let Err(e) = speaker
                .speak_for_show(speech, CancelSignal::new(), SpeechStartSignal::new())
                .await
            {
                tracing::info!(overlay = %overlay, reason = %e, "overlay speech did not play in full");
            }
        });
    }

    pub fn pending_shows(&self, id: &OverlayId) -> usize {
        self.inner.shows.depth(id)
    }

    pub fn watch_pending_shows(&self, id: &OverlayId) -> ShowDepthWatch {
        self.inner.shows.watch_depth(id)
    }

    pub fn clear_shows(&self, id: &OverlayId) -> usize {
        self.inner.shows.drop_pending(id, ShowEnd::Cleared)
    }

    fn withdraw_shows(&self, id: &OverlayId) {
        let withdrawn = self.inner.shows.drop_pending(id, ShowEnd::Withdrawn);
        if withdrawn > 0 {
            tracing::info!(overlay = %id, withdrawn, "queued overlay shows dropped with their overlay");
        }
    }

    pub async fn test_fire(
        &self,
        id: &OverlayId,
        samples: &dyn OverlaySampleSource,
    ) -> Result<TestFire, OverlayServiceError> {
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
            &samples.sample(&definition.id).await,
        );
        let disposition = descriptor.delivery_disposition();
        self.inner.bus.record(Event::new(
            EventSource::Core,
            OVERLAY_TEST_FIRE_KIND,
            json!({ OVERLAY_ID_KEY: definition.id.as_str() }),
        ));

        if latest_binding(descriptor, &definition.config).is_some() {
            let delivery = {
                let _lane = self.inner.lanes.enter(&definition.id).await;
                self.send(&definition.id, &content, None).await
            };
            self.restore_after_preview(definition.id);
            return Ok(TestFire { content, delivery });
        }

        let delivery = self
            .push(&definition.id, disposition, &content, None)
            .await?;

        Ok(TestFire { content, delivery })
    }

    fn restore_after_preview(&self, id: OverlayId) {
        let handle = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(LATEST_PREVIEW_HOLD).await;
            if let Err(error) = handle.replay_retained(&id).await {
                tracing::warn!(overlay = %id, %error, "latest value did not return after a preview");
            }
        });
    }

    pub async fn refresh_latest(&self, slot: Option<&str>) {
        let definitions = match self.inner.repo.list().await {
            Ok(definitions) => definitions,
            Err(error) => {
                tracing::warn!(%error, "overlays unreadable; latest values not pushed");
                return;
            }
        };
        for definition in definitions.iter().filter(|definition| definition.enabled) {
            let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
                continue;
            };
            let Some(binding) = latest_binding(descriptor, &definition.config) else {
                continue;
            };
            if slot.is_some_and(|changed| changed != binding.slot) {
                continue;
            }
            let _lane = self.inner.lanes.enter(&definition.id).await;
            if let Some(content) = self.live_latest_content(definition, descriptor) {
                self.send(&definition.id, &content, None).await;
            }
        }
    }

    fn live_latest_content(
        &self,
        definition: &OverlayDefinition,
        descriptor: &dyn OverlayKindDescriptor,
    ) -> Option<OverlayConfig> {
        let binding = latest_binding(descriptor, &definition.config)?;
        let value = self
            .inner
            .latest
            .as_ref()
            .and_then(|reader| reader.latest(&binding.slot, binding.scope()));
        Some(latest_content(
            descriptor,
            &definition.config,
            value.as_ref(),
        ))
    }

    async fn replay_retained(&self, id: &OverlayId) -> Result<(), OverlayServiceError> {
        let _lane = self.inner.lanes.enter(id).await;
        let definition = self.load(id).await?;
        let Some(descriptor) = self.inner.kinds.get(&definition.kind_id) else {
            return Ok(());
        };
        if let Some(content) = self.live_latest_content(&definition, descriptor) {
            self.send(id, &content, None).await;
            return Ok(());
        }
        if !descriptor.delivery_disposition().retains_last_content() {
            return Ok(());
        }
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
        self.inner
            .frames
            .deliver_content(id, content_json(content), duration_ms)
            .await
            .outcome()
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
        stored: &OverlayDefinition,
        samples: &dyn OverlaySampleSource,
    ) -> Result<MaterializeReport, OverlayServiceError> {
        let definition = &self.upgraded(stored);
        let root = self.root().await;
        let instance = instance_of(
            definition,
            self.media_pass(definition).await,
            samples.sample(&definition.id).await,
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

        let mut stamped = definition.clone();
        stamped.generator_version = GENERATOR_VERSION;
        if stamped != *stored {
            self.inner.repo.save(&stamped).await?;
        }

        Ok(report)
    }

    fn upgraded(&self, stored: &OverlayDefinition) -> OverlayDefinition {
        let mut current = stored.clone();
        let Some(descriptor) = self.inner.kinds.get(&stored.kind_id) else {
            return current;
        };
        if let Some(config) =
            upgrade_config(descriptor, stored.config_schema_version, &stored.config)
        {
            tracing::info!(
                overlay = %stored.id,
                from = stored.config_schema_version,
                to = descriptor.config_schema_version(),
                "overlay settings upgraded to the current layout"
            );
            current.config = config;
            current.config_schema_version = descriptor.config_schema_version();
        }
        current
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

#[async_trait]
impl OverlayConnectListener for OverlayServiceHandle {
    async fn overlay_connected(&self, identity: &OverlayId) {
        if let Err(error) = self.replay_retained(identity).await {
            tracing::warn!(overlay = %identity, %error, "retained overlay content did not replay");
        }
    }
}

pub(crate) fn content_json(content: &OverlayConfig) -> serde_json::Value {
    serde_json::Value::Object(
        content
            .iter()
            .map(|(key, value)| (key.clone(), value.to_plain_json()))
            .collect(),
    )
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
        credential: Some(definition.credential.clone()),
        media,
        sample,
    }
}

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
mod tests {
    use forge_storage::MockOverlayRepo;
    use forge_storage::test_support::MemorySettings;

    use super::*;
    use crate::NullEventLogRepo;

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

    fn handle_with(frames: Arc<dyn OverlayFrameSink>) -> OverlayServiceHandle {
        OverlayServiceHandle::new(OverlayServiceParts {
            repo: Arc::new(MockOverlayRepo::new()),
            settings: Arc::new(MemorySettings::new()),
            kinds: Arc::new(OverlayKindRegistry::new()),
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            frames,
            media: None,
            speaker: None,
            latest: None,
        })
    }

    #[tokio::test]
    async fn the_connected_page_count_comes_from_the_sink_for_the_identity_asked() {
        let connected = OverlayReceivers {
            sources: 2,
            preview_tabs: 1,
        };
        let handle = handle_with(Arc::new(CountingSink {
            identity: OverlayId::new(STAGE),
            connected,
        }));

        assert_eq!(handle.receivers(&OverlayId::new(STAGE)).await, connected);
        assert_eq!(
            handle.receivers(&OverlayId::new("alert-box")).await,
            OverlayReceivers::default(),
            "the handle asked the sink about an identity the caller never named"
        );
    }

    #[tokio::test]
    async fn no_pages_are_reported_when_the_sink_does_not_count_its_connections() {
        let handle = handle_with(Arc::new(SilentSink));

        assert_eq!(
            handle.receivers(&OverlayId::new(STAGE)).await,
            OverlayReceivers::default()
        );
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
        OverlayServiceHandle::new(OverlayServiceParts {
            repo: Arc::new(repo),
            settings: Arc::new(MemorySettings::new()),
            kinds: Arc::new(OverlayKindRegistry::new()),
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            frames: Arc::clone(log) as Arc<dyn OverlayFrameSink>,
            media: None,
            speaker: None,
            latest: None,
        })
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
