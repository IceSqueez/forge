use std::str::FromStr;
use std::sync::Arc;

use forge_overlay::config::SOUND;
use forge_overlay::{
    ClipReference, MEDIA_KEYS, MediaIssue, OverlayMedia, ResolvedMedia, clip_references,
};
use forge_storage::{
    MediaBlobId, MediaKind, MediaReferrer, MediaReferrerKind, MediaRepo, OverlayConfig, OverlayId,
    SoundboardClipsRepo, StorageError, clip_source_referrer,
};
use forge_types::ClipId;

const LIBRARY_UNREACHABLE: &str = "the media library is not available to the overlay service";

#[derive(Debug, Default)]
pub(crate) struct MediaPass {
    pub(crate) media: OverlayMedia,
    held: Vec<(String, MediaBlobId)>,
}

pub(crate) fn unresolvable(config: &OverlayConfig) -> MediaPass {
    MediaPass {
        media: OverlayMedia {
            resolved: Vec::new(),
            issues: clip_references(config)
                .into_iter()
                .map(|reference| MediaIssue::LookupFailed {
                    key: reference.key,
                    clip: reference.clip,
                    reason: LIBRARY_UNREACHABLE.to_owned(),
                })
                .collect(),
        },
        held: Vec::new(),
    }
}

#[derive(Clone)]
pub struct OverlayMediaLibrary {
    blobs: Arc<dyn MediaRepo>,
    clips: Arc<dyn SoundboardClipsRepo>,
}

impl OverlayMediaLibrary {
    pub fn new(blobs: Arc<dyn MediaRepo>, clips: Arc<dyn SoundboardClipsRepo>) -> Self {
        Self { blobs, clips }
    }

    pub(crate) async fn resolve(&self, config: &OverlayConfig) -> MediaPass {
        let mut pass = MediaPass::default();
        for reference in clip_references(config) {
            match self.resolve_one(&reference).await {
                Ok((blob, media)) => {
                    pass.held.push((reference.key, blob));
                    pass.media.resolved.push(media);
                }
                Err(issue) => pass.media.issues.push(issue),
            }
        }
        pass
    }

    pub(crate) async fn record(&self, overlay: &OverlayId, pass: &MediaPass) {
        for key in MEDIA_KEYS {
            if pass.media.issues.iter().any(|issue| {
                issue.key() == *key && matches!(issue, MediaIssue::LookupFailed { .. })
            }) {
                continue;
            }
            let desired = pass
                .held
                .iter()
                .find(|(held_key, _)| held_key == key)
                .map(|(_, blob)| blob);
            if let Err(error) = self.sync_slot(overlay, key, desired).await {
                tracing::warn!(overlay = %overlay, slot = key, %error, "overlay media reference not recorded");
            }
        }
    }

    pub(crate) async fn release(&self, overlay: &OverlayId) -> Result<u64, StorageError> {
        self.blobs
            .release_all(MediaReferrerKind::Overlay, overlay.as_str())
            .await
    }

    async fn sync_slot(
        &self,
        overlay: &OverlayId,
        key: &str,
        desired: Option<&MediaBlobId>,
    ) -> Result<(), StorageError> {
        let referrer = overlay_referrer(overlay, key);
        let current = self.blobs.blob_of(&referrer).await?;
        match (current, desired) {
            (Some(current), Some(desired)) if current == *desired => Ok(()),
            (None, None) => Ok(()),
            (_, Some(desired)) => self.blobs.retain(&referrer, desired).await,
            (Some(_), None) => self.blobs.release(&referrer).await.map(|_| ()),
        }
    }

    async fn resolve_one(
        &self,
        reference: &ClipReference,
    ) -> Result<(MediaBlobId, ResolvedMedia), MediaIssue> {
        let clip = ClipId::from_str(&reference.clip).map_err(|_| MediaIssue::UnknownClip {
            key: reference.key.clone(),
            clip: reference.clip.clone(),
        })?;

        let known = self
            .clips
            .get(clip)
            .await
            .map_err(|error| lookup_failed(reference, &error))?;
        if known.is_none() {
            return Err(MediaIssue::UnknownClip {
                key: reference.key.clone(),
                clip: reference.clip.clone(),
            });
        }

        let Some(id) = self
            .blobs
            .blob_of(&clip_source_referrer(clip))
            .await
            .map_err(|error| lookup_failed(reference, &error))?
        else {
            return Err(MediaIssue::ClipOutsideLibrary {
                key: reference.key.clone(),
                clip: reference.clip.clone(),
            });
        };

        let Some(blob) = self
            .blobs
            .get(&id)
            .await
            .map_err(|error| lookup_failed(reference, &error))?
        else {
            return Err(MediaIssue::ClipBytesMissing {
                key: reference.key.clone(),
                clip: reference.clip.clone(),
            });
        };

        if expected_kind(&reference.key).is_some_and(|expected| expected != blob.format.kind()) {
            return Err(MediaIssue::ClipKindMismatch {
                key: reference.key.clone(),
                clip: reference.clip.clone(),
                format: blob.format.to_string(),
            });
        }

        let bytes = match self.blobs.read(&id).await {
            Ok(bytes) => bytes,
            Err(StorageError::NotFound { .. }) => {
                return Err(MediaIssue::ClipBytesMissing {
                    key: reference.key.clone(),
                    clip: reference.clip.clone(),
                });
            }
            Err(error) => return Err(lookup_failed(reference, &error)),
        };

        let media = ResolvedMedia::new(
            reference.key.clone(),
            id.as_str(),
            blob.format.as_str(),
            bytes,
        )
        .map_err(|error| MediaIssue::LookupFailed {
            key: reference.key.clone(),
            clip: reference.clip.clone(),
            reason: error.to_string(),
        })?;

        Ok((id, media))
    }
}

fn overlay_referrer(overlay: &OverlayId, key: &str) -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::Overlay, overlay.as_str(), key)
}

fn expected_kind(key: &str) -> Option<MediaKind> {
    match key {
        SOUND => Some(MediaKind::Audio),
        _ => None,
    }
}

fn lookup_failed(reference: &ClipReference, error: &StorageError) -> MediaIssue {
    MediaIssue::LookupFailed {
        key: reference.key.clone(),
        clip: reference.clip.clone(),
        reason: error.to_string(),
    }
}
