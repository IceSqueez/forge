use forge_types::{ClipId, RecoverMutex, Shared};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_events::{EventPublisher, EventSource, TypedEvent};
use forge_storage::{
    MediaBlobId, MediaRefusal, MediaRepo, SoundboardClipsRepo, StorageError, StoredClip,
};

pub(crate) use forge_storage::clip_source_referrer;

use crate::error::SoundboardError;
use crate::vocabulary::{ClipAdoptionSettled, SettledVerdict};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipSource {
    Managed(PathBuf),
    Legacy(PathBuf),
    Missing,
}

impl ClipSource {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Managed(path) | Self::Legacy(path) => Some(path),
            Self::Missing => None,
        }
    }

    pub fn availability(&self, refusal: Option<MediaRefusal>) -> ClipAvailability {
        match self {
            Self::Managed(_) => ClipAvailability::Managed,
            Self::Legacy(_) => ClipAvailability::Unadopted { refusal },
            Self::Missing => ClipAvailability::Missing,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipAvailability {
    Managed,
    Unadopted { refusal: Option<MediaRefusal> },
    Missing,
}

impl ClipAvailability {
    pub const fn is_playable(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionVerdict {
    Adopted,
    AlreadyManaged,
    InFlight,
    SourceMissing,
    Refused(MediaRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdoptionStep {
    Import(PathBuf),
    Settled(AdoptionVerdict),
}

pub(crate) fn plan_adoption(source: ClipSource) -> AdoptionStep {
    match source {
        ClipSource::Managed(_) => AdoptionStep::Settled(AdoptionVerdict::AlreadyManaged),
        ClipSource::Legacy(path) => AdoptionStep::Import(path),
        ClipSource::Missing => AdoptionStep::Settled(AdoptionVerdict::SourceMissing),
    }
}

pub(crate) fn choose_source(managed: Option<PathBuf>, legacy: Option<PathBuf>) -> ClipSource {
    match (managed, legacy) {
        (Some(path), _) => ClipSource::Managed(path),
        (None, Some(path)) => ClipSource::Legacy(path),
        (None, None) => ClipSource::Missing,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourcePlan {
    ImportChosen,
    ImportAdopted,
    Keep,
}

pub(crate) fn plan_source(previous: Option<&Path>, next: &Path, resolvable: bool) -> SourcePlan {
    match previous {
        None => SourcePlan::ImportChosen,
        Some(previous) if previous != next => SourcePlan::ImportChosen,
        Some(_) if !resolvable => SourcePlan::ImportAdopted,
        Some(_) => SourcePlan::Keep,
    }
}

pub(crate) fn final_refusal(error: &StorageError) -> Option<MediaRefusal> {
    match error {
        StorageError::MediaRefused(refusal) => Some(refusal.clone()),
        _ => None,
    }
}

fn storage_failed(error: StorageError) -> SoundboardError {
    SoundboardError::Storage(error)
}

fn import_failed(error: StorageError) -> SoundboardError {
    match error {
        StorageError::MediaRefused(refusal) => SoundboardError::ImportRefused(refusal),
        other => SoundboardError::Storage(other),
    }
}

fn report_adoption(clip_id: ClipId, outcome: &Result<AdoptionVerdict, SoundboardError>) {
    match outcome {
        Ok(AdoptionVerdict::Adopted) => {}
        Ok(verdict) => {
            tracing::warn!(clip_id = %clip_id, verdict = ?verdict, "clip source stays outside the media library");
        }
        Err(error) => {
            tracing::warn!(clip_id = %clip_id, error = %error, "clip source stays outside the media library");
        }
    }
}

async fn readable_all(paths: Vec<PathBuf>) -> Vec<Option<PathBuf>> {
    let count = paths.len();
    tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| path.is_file().then_some(path))
            .collect()
    })
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(error = %error, "clip source probe failed");
        vec![None; count]
    })
}

async fn readable(path: PathBuf) -> Option<PathBuf> {
    readable_all(vec![path]).await.into_iter().next().flatten()
}

pub struct ClipLibrary {
    clips: Arc<dyn SoundboardClipsRepo>,
    media: Arc<dyn MediaRepo>,
    adopting: Mutex<HashSet<ClipId>>,
    refused: Mutex<HashMap<ClipId, MediaRefusal>>,
    publisher: Shared<Option<Arc<dyn EventPublisher>>>,
}

impl ClipLibrary {
    pub fn new(clips: Arc<dyn SoundboardClipsRepo>, media: Arc<dyn MediaRepo>) -> Self {
        Self {
            clips,
            media,
            adopting: Mutex::new(HashSet::new()),
            refused: Mutex::new(HashMap::new()),
            publisher: Shared::new(None),
        }
    }

    pub fn install_event_publisher(&self, publisher: Arc<dyn EventPublisher>) {
        self.publisher.store(Some(publisher));
    }

    pub async fn list(&self) -> Result<Vec<StoredClip>, SoundboardError> {
        self.clips.list().await.map_err(storage_failed)
    }

    pub async fn get(&self, id: ClipId) -> Result<Option<StoredClip>, SoundboardError> {
        self.clips.get(id).await.map_err(storage_failed)
    }

    pub async fn total_bytes(&self) -> Result<u64, SoundboardError> {
        self.media.total_bytes().await.map_err(storage_failed)
    }

    pub async fn save_clip(&self, clip: &StoredClip) -> Result<(), SoundboardError> {
        let previous = self.clips.get(clip.id).await.map_err(storage_failed)?;
        let resolvable = self.managed_path(clip.id).await.is_some();
        let plan = plan_source(
            previous.as_ref().map(|row| row.file_path.as_path()),
            &clip.file_path,
            resolvable,
        );

        let imported = match plan {
            SourcePlan::ImportChosen => Some(
                self.media
                    .import_file(&clip.file_path)
                    .await
                    .map_err(import_failed)?,
            ),
            SourcePlan::ImportAdopted => {
                let outcome = self.adopt_source(clip.id, &clip.file_path).await;
                report_adoption(clip.id, &outcome);
                None
            }
            SourcePlan::Keep => None,
        };

        let Some(blob) = imported else {
            return self.clips.save(clip).await.map_err(storage_failed);
        };
        if let Err(error) = self
            .clips
            .save_with_source(clip, &blob.id, Arc::clone(&self.media))
            .await
        {
            self.discard_unreferenced(&blob.id).await;
            return Err(storage_failed(error));
        }
        self.clear_refusal(clip.id);
        Ok(())
    }

    async fn discard_unreferenced(&self, blob: &MediaBlobId) {
        match self.media.delete(blob).await {
            Ok(_) | Err(StorageError::MediaReferenced { .. }) => {}
            Err(error) => {
                tracing::warn!(%blob, %error, "imported clip audio left without a clip");
            }
        }
    }

    pub async fn record_probed_duration(
        &self,
        id: ClipId,
        probed_file: &Path,
        duration_secs: f32,
    ) -> Result<(), SoundboardError> {
        let Some(mut current) = self.clips.get(id).await.map_err(storage_failed)? else {
            return Ok(());
        };
        if current.file_path != probed_file || current.duration_secs.is_some() {
            return Ok(());
        }
        current.duration_secs = Some(duration_secs);
        self.clips.save(&current).await.map_err(storage_failed)
    }

    pub async fn delete_clip(&self, id: ClipId) -> Result<bool, SoundboardError> {
        let deleted = self
            .clips
            .delete_with_sources(id, Arc::clone(&self.media))
            .await
            .map_err(storage_failed)?;
        self.clear_refusal(id);
        Ok(deleted)
    }

    pub async fn source_of(&self, clip: &StoredClip) -> ClipSource {
        let managed = self.managed_path(clip.id).await;
        let legacy = readable(clip.file_path.clone()).await;
        choose_source(managed, legacy)
    }

    pub async fn availability(&self, clip: &StoredClip) -> ClipAvailability {
        self.source_of(clip)
            .await
            .availability(self.recorded_refusal(clip.id))
    }

    pub async fn availability_of(&self, clips: &[StoredClip]) -> Vec<(ClipId, ClipAvailability)> {
        let mut managed = Vec::with_capacity(clips.len());
        for clip in clips {
            managed.push(self.managed_path(clip.id).await);
        }
        let legacy = readable_all(clips.iter().map(|clip| clip.file_path.clone()).collect()).await;

        clips
            .iter()
            .zip(managed)
            .zip(legacy)
            .map(|((clip, managed), legacy)| {
                let availability =
                    choose_source(managed, legacy).availability(self.recorded_refusal(clip.id));
                (clip.id, availability)
            })
            .collect()
    }

    pub fn adopt_in_background(self: &Arc<Self>, clip_id: ClipId, source: PathBuf) {
        if self.recorded_refusal(clip_id).is_some() || !self.claim_adoption(clip_id) {
            return;
        }
        let library = Arc::clone(self);
        tokio::spawn(async move {
            let outcome = library.import_source(clip_id, &source).await;
            report_adoption(clip_id, &outcome);
            library.release_claim(clip_id);
        });
    }

    pub async fn adopt_now(&self, clip: &StoredClip) -> Result<AdoptionVerdict, SoundboardError> {
        match plan_adoption(self.source_of(clip).await) {
            AdoptionStep::Settled(verdict) => {
                self.publish_settled(clip.id, &verdict);
                Ok(verdict)
            }
            AdoptionStep::Import(path) => self.adopt_source(clip.id, &path).await,
        }
    }

    pub async fn adopt_all_now(
        &self,
        clips: &[StoredClip],
    ) -> Vec<(ClipId, Result<AdoptionVerdict, SoundboardError>)> {
        let mut verdicts = Vec::with_capacity(clips.len());
        for clip in clips {
            verdicts.push((clip.id, self.adopt_now(clip).await));
        }
        verdicts
    }

    async fn adopt_source(
        &self,
        clip_id: ClipId,
        source: &Path,
    ) -> Result<AdoptionVerdict, SoundboardError> {
        if !self.claim_adoption(clip_id) {
            return Ok(AdoptionVerdict::InFlight);
        }
        let outcome = self.import_source(clip_id, source).await;
        self.release_claim(clip_id);
        outcome
    }

    async fn import_source(
        &self,
        clip_id: ClipId,
        source: &Path,
    ) -> Result<AdoptionVerdict, SoundboardError> {
        let verdict = match self.media.import_file(source).await {
            Ok(blob) => {
                self.retain(clip_id, &blob.id).await?;
                self.clear_refusal(clip_id);
                AdoptionVerdict::Adopted
            }
            Err(error) => match final_refusal(&error) {
                Some(refusal) => {
                    self.record_refusal(clip_id, refusal.clone());
                    AdoptionVerdict::Refused(refusal)
                }
                None => return Err(storage_failed(error)),
            },
        };
        self.publish_settled(clip_id, &verdict);
        Ok(verdict)
    }

    fn publish_settled(&self, clip_id: ClipId, verdict: &AdoptionVerdict) {
        let verdict = match verdict {
            AdoptionVerdict::InFlight => return,
            AdoptionVerdict::Adopted => SettledVerdict::Adopted,
            AdoptionVerdict::AlreadyManaged => SettledVerdict::AlreadyManaged,
            AdoptionVerdict::SourceMissing => SettledVerdict::SourceMissing,
            AdoptionVerdict::Refused(refusal) => SettledVerdict::Refused {
                reason: refusal.to_string(),
            },
        };
        let snapshot = self.publisher.load();
        if let Some(publisher) = snapshot.as_ref() {
            ClipAdoptionSettled { clip_id, verdict }.publish(
                publisher.as_ref(),
                EventSource::Audio,
                None,
            );
        }
    }

    async fn managed_path(&self, clip_id: ClipId) -> Option<PathBuf> {
        let referrer = clip_source_referrer(clip_id);
        let blob = match self.media.blob_of(&referrer).await {
            Ok(Some(blob)) => blob,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(clip_id = %clip_id, error = %error, "clip media reference lookup failed");
                return None;
            }
        };
        match self.media.resolve(&blob).await {
            Ok(path) => Some(path),
            Err(StorageError::NotFound { .. }) => None,
            Err(error) => {
                tracing::warn!(clip_id = %clip_id, error = %error, "clip media resolution failed");
                None
            }
        }
    }

    async fn retain(&self, clip_id: ClipId, blob: &MediaBlobId) -> Result<(), SoundboardError> {
        let referrer = clip_source_referrer(clip_id);
        self.media
            .retain(&referrer, blob)
            .await
            .map_err(storage_failed)
    }

    fn claim_adoption(&self, clip_id: ClipId) -> bool {
        self.adopting.lock_recover().insert(clip_id)
    }

    fn release_claim(&self, clip_id: ClipId) {
        self.adopting.lock_recover().remove(&clip_id);
    }

    fn record_refusal(&self, clip_id: ClipId, refusal: MediaRefusal) {
        self.refused.lock_recover().insert(clip_id, refusal);
    }

    fn recorded_refusal(&self, clip_id: ClipId) -> Option<MediaRefusal> {
        self.refused.lock_recover().get(&clip_id).cloned()
    }

    fn clear_refusal(&self, clip_id: ClipId) {
        self.refused.lock_recover().remove(&clip_id);
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::{MediaFormat, MediaKind};

    use super::*;

    const MANAGED: &str = "/data/media/sha256-abc.wav";
    const LEGACY: &str = "/home/streamer/sounds/fanfare.wav";
    const OTHER_LEGACY: &str = "/home/streamer/sounds/airhorn.wav";

    fn path(raw: &str) -> PathBuf {
        PathBuf::from(raw)
    }

    #[test]
    fn choose_source_prefers_the_managed_copy_and_falls_back_to_the_legacy_path() {
        for (managed, legacy, expected) in [
            (
                Some(path(MANAGED)),
                Some(path(LEGACY)),
                ClipSource::Managed(path(MANAGED)),
            ),
            (
                Some(path(MANAGED)),
                None,
                ClipSource::Managed(path(MANAGED)),
            ),
            (None, Some(path(LEGACY)), ClipSource::Legacy(path(LEGACY))),
            (None, None, ClipSource::Missing),
        ] {
            assert_eq!(choose_source(managed.clone(), legacy.clone()), expected);
        }
    }

    #[test]
    fn a_source_that_resolves_nowhere_is_the_only_unplayable_one() {
        for (source, playable) in [
            (ClipSource::Managed(path(MANAGED)), true),
            (ClipSource::Legacy(path(LEGACY)), true),
            (ClipSource::Missing, false),
        ] {
            assert_eq!(
                source.availability(None).is_playable(),
                playable,
                "{source:?}"
            );
        }
    }

    #[test]
    fn plan_source_imports_what_the_user_named_and_adopts_only_an_untouched_row() {
        for (previous, next, resolvable, expected) in [
            (None, LEGACY, false, SourcePlan::ImportChosen),
            (None, LEGACY, true, SourcePlan::ImportChosen),
            (Some(OTHER_LEGACY), LEGACY, true, SourcePlan::ImportChosen),
            (Some(OTHER_LEGACY), LEGACY, false, SourcePlan::ImportChosen),
            (Some(LEGACY), LEGACY, false, SourcePlan::ImportAdopted),
            (Some(LEGACY), LEGACY, true, SourcePlan::Keep),
        ] {
            let previous = previous.map(path);
            assert_eq!(
                plan_source(previous.as_deref(), &path(next), resolvable),
                expected,
                "previous {previous:?} next {next} resolvable {resolvable}"
            );
        }
    }

    #[test]
    fn only_an_admission_refusal_is_final_and_it_carries_the_reason_verbatim() {
        for (error, expected) in [
            (
                StorageError::MediaRefused(MediaRefusal::Unsupported {
                    label: "notes.txt".to_owned(),
                }),
                Some(MediaRefusal::Unsupported {
                    label: "notes.txt".to_owned(),
                }),
            ),
            (
                StorageError::MediaRefused(MediaRefusal::TypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                }),
                Some(MediaRefusal::TypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                }),
            ),
            (
                StorageError::MediaRefused(MediaRefusal::TooLarge {
                    label: "huge.wav".to_owned(),
                    size: 2,
                    limit: 1,
                    kind: MediaKind::Audio,
                }),
                Some(MediaRefusal::TooLarge {
                    label: "huge.wav".to_owned(),
                    size: 2,
                    limit: 1,
                    kind: MediaKind::Audio,
                }),
            ),
            (
                StorageError::NotFound {
                    key: "sha256-abc".to_owned(),
                },
                None,
            ),
            (StorageError::MediaReferenced { referrer_count: 1 }, None),
            (
                StorageError::Connection {
                    reason: "disk is busy".to_owned(),
                },
                None,
            ),
            (
                StorageError::Io(std::io::Error::other("device went away")),
                None,
            ),
        ] {
            assert_eq!(final_refusal(&error), expected, "{error}");
        }
    }
}
