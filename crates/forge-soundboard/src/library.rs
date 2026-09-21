use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use forge_storage::{
    MediaBlobId, MediaReferrer, MediaReferrerKind, MediaRepo, SoundboardClipsRepo, StorageError,
    StoredClip,
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

    pub const fn availability(&self) -> ClipAvailability {
        match self {
            Self::Managed(_) => ClipAvailability::Managed,
            Self::Legacy(_) => ClipAvailability::Unadopted,
            Self::Missing => ClipAvailability::Missing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipAvailability {
    Managed,
    Unadopted,
    Missing,
}

impl ClipAvailability {
    pub const fn is_playable(self) -> bool {
        !matches!(self, Self::Missing)
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
pub fn refusal_is_final(error: &StorageError) -> bool {
    matches!(
        error,
        StorageError::MediaUnsupported { .. }
            | StorageError::MediaTypeMismatch { .. }
            | StorageError::MediaTooLarge { .. }
    )
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
    refused: Mutex<HashSet<ClipId>>,
}

impl ClipLibrary {
    pub fn new(clips: Arc<dyn SoundboardClipsRepo>, media: Arc<dyn MediaRepo>) -> Self {
        Self {
            clips,
            media,
            adopting: Mutex::new(HashSet::new()),
            refused: Mutex::new(HashSet::new()),
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
            SourcePlan::ImportAdopted => match self.media.import_file(&clip.file_path).await {
                Ok(blob) => Some(blob),
                Err(error) => {
                    self.note_refusal(clip.id, &error);
                    tracing::warn!(clip_id = %clip.id, error = %error, "clip source stays outside the media library");
                    None
                }
            },
            SourcePlan::Keep => None,
        };

        self.clips.save(clip).await.map_err(storage_failed)?;

        if let Some(blob) = imported {
            self.retain(clip.id, &blob.id).await;
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
        self.source_of(clip).await.availability()
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
                (clip.id, choose_source(managed, legacy).availability())
            })
            .collect()
    }

    pub fn adopt_in_background(self: &Arc<Self>, clip_id: ClipId, source: PathBuf) {
        if !self.claim_adoption(clip_id) {
            return;
        }
        let library = Arc::clone(self);
        tokio::spawn(async move {
            match library.media.import_file(&source).await {
                Ok(blob) => {
                    library.retain(clip_id, &blob.id).await;
                    library.clear_refusal(clip_id);
                }
                Err(error) => {
                    library.note_refusal(clip_id, &error);
                    tracing::warn!(clip_id = %clip_id, error = %error, "clip source stays outside the media library");
                }
            }
            library.release_claim(clip_id);
        });
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

    async fn retain(&self, clip_id: ClipId, blob: &MediaBlobId) {
        let referrer = clip_source_referrer(clip_id);
        if let Err(error) = self.media.retain(&referrer, blob).await {
            tracing::warn!(clip_id = %clip_id, error = %error, "clip media reference was not recorded");
        }
    }

    fn claim_adoption(&self, clip_id: ClipId) -> bool {
        if self
            .refused
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&clip_id)
        {
            return false;
        }
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

    fn note_refusal(&self, clip_id: ClipId, error: &StorageError) {
        if refusal_is_final(error) {
            self.refused
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(clip_id);
        }
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
            assert_eq!(source.availability().is_playable(), playable, "{source:?}");
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
    fn only_an_admission_refusal_counts_as_final() {
        for (error, final_refusal) in [
            (
                StorageError::MediaUnsupported {
                    label: "notes.txt".to_owned(),
                },
                true,
            ),
            (
                StorageError::MediaTypeMismatch {
                    label: "logo.mp3".to_owned(),
                    claimed: MediaFormat::Mp3,
                    detected: MediaFormat::Png,
                },
                true,
            ),
            (
                StorageError::MediaTooLarge {
                    label: "huge.wav".to_owned(),
                    size: 2,
                    limit: 1,
                    kind: MediaKind::Audio,
                },
                true,
            ),
            (
                StorageError::NotFound {
                    key: "sha256-abc".to_owned(),
                },
                false,
            ),
            (StorageError::MediaReferenced { referrer_count: 1 }, false),
            (
                StorageError::Connection {
                    reason: "disk is busy".to_owned(),
                },
                false,
            ),
            (
                StorageError::Io(std::io::Error::other("device went away")),
                false,
            ),
        ] {
            assert_eq!(refusal_is_final(&error), final_refusal, "{error}");
        }
    }
}
