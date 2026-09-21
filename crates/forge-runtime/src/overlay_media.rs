use std::str::FromStr;
use std::sync::Arc;

use forge_overlay::{
    ClipReference, ImageReference, MEDIA_KEYS, MediaIssue, OverlayMedia, ResolvedMedia,
    clip_references, glyph_media, image_references,
};
use forge_storage::{
    MediaBlobId, MediaFormat, MediaKind, MediaReferrer, MediaReferrerKind, MediaRepo,
    OverlayConfig, OverlayId, SoundboardClipsRepo, StorageError, clip_source_referrer,
};
use forge_types::ClipId;

const LIBRARY_UNREACHABLE: &str = "the media library is not available to the overlay service";

#[derive(Debug, Default)]
pub(crate) struct MediaPass {
    pub(crate) media: OverlayMedia,
    held: Vec<(String, MediaBlobId)>,
}

pub(crate) fn unresolvable(config: &OverlayConfig) -> MediaPass {
    let mut media = glyph_media(config);
    for reference in clip_references(config) {
        media.issues.push(MediaIssue::LookupFailed {
            key: reference.key,
            reference: reference.clip,
            reason: LIBRARY_UNREACHABLE.to_owned(),
        });
    }
    for reference in image_references(config) {
        media.issues.push(MediaIssue::LookupFailed {
            key: reference.key,
            reference: reference.image,
            reason: LIBRARY_UNREACHABLE.to_owned(),
        });
    }
    MediaPass {
        media,
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
        let mut pass = MediaPass {
            media: glyph_media(config),
            held: Vec::new(),
        };
        for reference in clip_references(config) {
            match self.resolve_clip(&reference).await {
                Ok((blob, media)) => {
                    pass.held.push((reference.key, blob));
                    pass.media.resolved.push(media);
                }
                Err(issue) => pass.media.issues.push(issue),
            }
        }
        for reference in image_references(config) {
            match self.resolve_image(&reference).await {
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

    async fn resolve_clip(
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
            .map_err(|error| lookup_failed(&reference.key, &reference.clip, &error))?;
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
            .map_err(|error| lookup_failed(&reference.key, &reference.clip, &error))?
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
            .map_err(|error| lookup_failed(&reference.key, &reference.clip, &error))?
        else {
            return Err(MediaIssue::ClipBytesMissing {
                key: reference.key.clone(),
                clip: reference.clip.clone(),
            });
        };

        if blob.format.kind() != MediaKind::Audio {
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
            Err(error) => return Err(lookup_failed(&reference.key, &reference.clip, &error)),
        };

        let media = named(&reference.key, &reference.clip, &id, blob.format, bytes)?;
        Ok((id, media))
    }

    async fn resolve_image(
        &self,
        reference: &ImageReference,
    ) -> Result<(MediaBlobId, ResolvedMedia), MediaIssue> {
        let id = MediaBlobId::from_stored(reference.image.clone());

        let Some(blob) = self
            .blobs
            .get(&id)
            .await
            .map_err(|error| lookup_failed(&reference.key, &reference.image, &error))?
        else {
            return Err(MediaIssue::UnknownImage {
                key: reference.key.clone(),
                image: reference.image.clone(),
            });
        };

        if blob.format.kind() != MediaKind::Image {
            return Err(MediaIssue::ImageKindMismatch {
                key: reference.key.clone(),
                image: reference.image.clone(),
                format: blob.format.to_string(),
            });
        }

        let bytes = match self.blobs.read(&id).await {
            Ok(bytes) => bytes,
            Err(StorageError::NotFound { .. }) => {
                return Err(MediaIssue::ImageBytesMissing {
                    key: reference.key.clone(),
                    image: reference.image.clone(),
                });
            }
            Err(error) => return Err(lookup_failed(&reference.key, &reference.image, &error)),
        };

        let media = named(&reference.key, &reference.image, &id, blob.format, bytes)?;
        Ok((id, media))
    }
}

fn named(
    key: &str,
    reference: &str,
    id: &MediaBlobId,
    format: MediaFormat,
    bytes: Vec<u8>,
) -> Result<ResolvedMedia, MediaIssue> {
    ResolvedMedia::new(key, id.as_str(), format.as_str(), bytes).map_err(|error| {
        MediaIssue::LookupFailed {
            key: key.to_owned(),
            reference: reference.to_owned(),
            reason: error.to_string(),
        }
    })
}

fn overlay_referrer(overlay: &OverlayId, key: &str) -> MediaReferrer {
    MediaReferrer::new(MediaReferrerKind::Overlay, overlay.as_str(), key)
}

fn lookup_failed(key: &str, reference: &str, error: &StorageError) -> MediaIssue {
    MediaIssue::LookupFailed {
        key: key.to_owned(),
        reference: reference.to_owned(),
        reason: error.to_string(),
    }
}
