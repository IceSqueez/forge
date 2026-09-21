use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use forge_storage::{
    MediaBlobId, MediaFormat, MediaKind, MediaReferrer, MediaReferrerKind, MediaRepo,
    SoundboardClipsRepo, StorageError, StoredClip,
};
use forge_types::ClipId;

use crate::error::SoundboardError;

const CLIP_SOURCE_SLOT: &str = "source";

pub fn clip_source_referrer(clip_id: ClipId) -> MediaReferrer {
    MediaReferrer::new(
        MediaReferrerKind::SoundboardClip,
        clip_id.to_string(),
        CLIP_SOURCE_SLOT,
    )
}

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

    pub fn availability(&self, refusal: Option<ClipRefusal>) -> ClipAvailability {
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
    Unadopted { refusal: Option<ClipRefusal> },
    Missing,
}

impl ClipAvailability {
    pub const fn is_playable(&self) -> bool {
        !matches!(self, Self::Missing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipRefusal {
    Unsupported {
        label: String,
    },
    TypeMismatch {
        label: String,
        claimed: MediaFormat,
        detected: MediaFormat,
    },
    TooLarge {
        label: String,
        size: u64,
        limit: u64,
        kind: MediaKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionVerdict {
    Adopted,
    AlreadyManaged,
    InFlight,
    SourceMissing,
    Refused(ClipRefusal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionStep {
    Import(PathBuf),
    Settled(AdoptionVerdict),
}

pub fn plan_adoption(source: ClipSource) -> AdoptionStep {
    match source {
        ClipSource::Managed(_) => AdoptionStep::Settled(AdoptionVerdict::AlreadyManaged),
        ClipSource::Legacy(path) => AdoptionStep::Import(path),
        ClipSource::Missing => AdoptionStep::Settled(AdoptionVerdict::SourceMissing),
    }
}

/// Both arguments are paths already verified readable; a vanished file arrives as `None`.
pub fn choose_source(managed: Option<PathBuf>, legacy: Option<PathBuf>) -> ClipSource {
    match (managed, legacy) {
        (Some(path), _) => ClipSource::Managed(path),
        (None, Some(path)) => ClipSource::Legacy(path),
        (None, None) => ClipSource::Missing,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePlan {
    /// The user named this file in this save, so a refusal must fail the save.
    ImportChosen,
    /// Opportunistic adoption of an untouched row, so a refusal leaves the row playable.
    ImportAdopted,
    Keep,
}

pub fn plan_source(previous: Option<&Path>, next: &Path, resolvable: bool) -> SourcePlan {
    match previous {
        None => SourcePlan::ImportChosen,
        Some(previous) if previous != next => SourcePlan::ImportChosen,
        Some(_) if !resolvable => SourcePlan::ImportAdopted,
        Some(_) => SourcePlan::Keep,
    }
}

/// Refusals the same bytes would produce again; every other failure may be transient.
pub fn final_refusal(error: &StorageError) -> Option<ClipRefusal> {
    match error {
        StorageError::MediaUnsupported { label } => Some(ClipRefusal::Unsupported {
            label: label.clone(),
        }),
        StorageError::MediaTypeMismatch {
            label,
            claimed,
            detected,
        } => Some(ClipRefusal::TypeMismatch {
            label: label.clone(),
            claimed: *claimed,
            detected: *detected,
        }),
        StorageError::MediaTooLarge {
            label,
            size,
            limit,
            kind,
        } => Some(ClipRefusal::TooLarge {
            label: label.clone(),
            size: *size,
            limit: *limit,
            kind: *kind,
        }),
        _ => None,
    }
}

pub fn refusal_is_final(error: &StorageError) -> bool {
    final_refusal(error).is_some()
}

fn storage_failed(error: StorageError) -> SoundboardError {
    SoundboardError::Storage(error.to_string())
}

fn import_failed(error: StorageError) -> SoundboardError {
    if refusal_is_final(&error) {
        SoundboardError::ImportRefused(error)
    } else {
        SoundboardError::Storage(error.to_string())
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
    refused: Mutex<HashMap<ClipId, ClipRefusal>>,
}

impl ClipLibrary {
    pub fn new(clips: Arc<dyn SoundboardClipsRepo>, media: Arc<dyn MediaRepo>) -> Self {
        Self {
            clips,
            media,
            adopting: Mutex::new(HashSet::new()),
            refused: Mutex::new(HashMap::new()),
        }
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

    /// Imports the row's file before the row is written, so a refused import stores nothing.
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

        self.clips.save(clip).await.map_err(storage_failed)?;

        if let Some(blob) = imported {
            self.retain(clip.id, &blob.id).await?;
            self.clear_refusal(clip.id);
        }
        Ok(())
    }

    /// Drops the row's library reference; collecting an unreferenced blob is a separate sweep.
    pub async fn delete_clip(&self, id: ClipId) -> Result<bool, SoundboardError> {
        self.media
            .release_all(MediaReferrerKind::SoundboardClip, &id.to_string())
            .await
            .map_err(storage_failed)?;
        self.clear_refusal(id);
        self.clips.delete(id).await.map_err(storage_failed)
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
            AdoptionStep::Settled(verdict) => Ok(verdict),
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
        match self.media.import_file(source).await {
            Ok(blob) => {
                self.retain(clip_id, &blob.id).await?;
                self.clear_refusal(clip_id);
                Ok(AdoptionVerdict::Adopted)
            }
            Err(error) => match final_refusal(&error) {
                Some(refusal) => {
                    self.record_refusal(clip_id, refusal.clone());
                    Ok(AdoptionVerdict::Refused(refusal))
                }
                None => Err(storage_failed(error)),
            },
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
        self.adopting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(clip_id)
    }

    fn release_claim(&self, clip_id: ClipId) {
        self.adopting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&clip_id);
    }

    fn record_refusal(&self, clip_id: ClipId, refusal: ClipRefusal) {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(clip_id, refusal);
    }

    fn recorded_refusal(&self, clip_id: ClipId) -> Option<ClipRefusal> {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&clip_id)
            .cloned()
    }

    fn clear_refusal(&self, clip_id: ClipId) {
        self.refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&clip_id);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
                StorageError::MediaUnsupported {
                    label: "notes.txt".to_owned(),
                },
                Some(ClipRefusal::Unsupported {
                    label: "notes.txt".to_owned(),
                }),
            ),
            (
                StorageError::MediaTypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                },
                Some(ClipRefusal::TypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                }),
            ),
            (
                StorageError::MediaTooLarge {
                    label: "huge.wav".to_owned(),
                    size: 2,
                    limit: 1,
                    kind: MediaKind::Audio,
                },
                Some(ClipRefusal::TooLarge {
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
