use std::collections::BTreeSet;
use std::path::PathBuf;

use forge_overlay::image_references;
use forge_registry::{InternalNamespace, ReferenceRemap};
use forge_storage::{
    DataProvider, MediaBlobId, MediaReferrer, MediaReferrerKind, OverlayConfig, OverlayId,
    QueueRole, ScriptRecord, StoreChange, StoredClip, clip_source_referrer,
};
use forge_types::{ClipId, OutputDevice, Queue, QueueId, ScriptId};
use time::OffsetDateTime;

use crate::documents::inventory::Stored;
use crate::documents::model::{ClipDocument, ScriptDocument};
use crate::documents::undo::{Undo, UndoJournal};
use crate::error::TransferError;

pub(crate) struct MediaIntake<'a> {
    pub(crate) provider: &'a dyn DataProvider,
    pub(crate) known_media: &'a mut BTreeSet<MediaBlobId>,
    pub(crate) undo: &'a mut UndoJournal,
}

impl MediaIntake<'_> {
    pub(crate) async fn store(
        &mut self,
        source: PathBuf,
        label: &str,
    ) -> Result<MediaBlobId, TransferError> {
        let bytes = tokio::fs::read(&source).await?;
        let blob = self.provider.media_repo().store(label, bytes).await?;
        if self.known_media.insert(blob.id.clone()) {
            self.undo.push(Undo::Media(blob.id.clone()));
        }
        Ok(blob.id)
    }
}

pub(crate) fn staged_source(
    located: Option<PathBuf>,
    document_path: &str,
) -> Result<PathBuf, TransferError> {
    located.ok_or_else(|| TransferError::ContentsMismatch {
        path: document_path.to_owned(),
    })
}

pub(crate) fn remapped_media(
    remap: &ReferenceRemap,
    media_id: &str,
) -> Result<MediaBlobId, TransferError> {
    let stored = remap
        .get(InternalNamespace::ManagedMedia, media_id)
        .unwrap_or(media_id);
    Ok(MediaBlobId::from_stored(stored)?)
}

pub(crate) struct ClipTarget<'a> {
    pub(crate) id: ClipId,
    pub(crate) name: &'a str,
    pub(crate) previous: Option<&'a StoredClip>,
    pub(crate) hotkey: Option<String>,
}

pub(crate) async fn clip_changes(
    provider: &dyn DataProvider,
    document: &ClipDocument,
    target: ClipTarget<'_>,
    blob: Option<MediaBlobId>,
) -> Result<Vec<StoreChange>, TransferError> {
    let ClipTarget {
        id,
        name,
        previous,
        hotkey,
    } = target;
    let file_path = match &blob {
        Some(blob) => provider.media_repo().resolve(blob).await?,
        None => previous
            .map(|clip| clip.file_path.clone())
            .unwrap_or_default(),
    };
    let clip = StoredClip {
        id,
        name: name.to_owned(),
        file_path,
        volume: document.volume,
        output_device: previous.map_or(OutputDevice::Default, |clip| clip.output_device.clone()),
        hotkey,
        created_at: previous.map_or_else(OffsetDateTime::now_utc, |clip| clip.created_at),
        category: document.category.clone(),
        loop_playback: document.loop_playback,
        duration_secs: document.duration_secs,
        builtin_id: document.builtin_id.clone(),
    };
    let mut changes = vec![StoreChange::SaveClip(Box::new(clip))];
    if let Some(blob) = blob {
        changes.push(StoreChange::RetainMedia {
            referrer: clip_source_referrer(id),
            blob,
        });
    }
    Ok(changes)
}

pub(crate) fn script_changes(
    document: &ScriptDocument,
    id: ScriptId,
    name: &str,
    previous: Option<Stored<ScriptRecord>>,
) -> Vec<StoreChange> {
    let archived = previous.as_ref().is_some_and(|stored| stored.archived);
    let now = OffsetDateTime::now_utc();
    let record = ScriptRecord {
        id,
        name: name.to_owned(),
        body: document.body.clone(),
        contract: document.contract.clone(),
        enabled: document.enabled,
        created_at: previous.map_or(now, |stored| stored.row.created_at),
        last_modified: now,
    };
    let mut changes = vec![StoreChange::SaveScript(Box::new(record))];
    if archived {
        changes.push(StoreChange::RestoreScript(id));
    }
    changes
}

pub(crate) fn overlay_media_retains(
    config: &OverlayConfig,
    id: &OverlayId,
    known_media: &BTreeSet<MediaBlobId>,
) -> Vec<StoreChange> {
    image_references(config)
        .into_iter()
        .filter_map(|reference| {
            let blob = MediaBlobId::from_stored(reference.image).ok()?;
            known_media
                .contains(&blob)
                .then(|| StoreChange::RetainMedia {
                    referrer: MediaReferrer::new(
                        MediaReferrerKind::Overlay,
                        id.as_str(),
                        reference.key,
                    ),
                    blob,
                })
        })
        .collect()
}

pub(crate) fn queue_changes(
    queue: Queue,
    role: Option<QueueRole>,
    claim: fn(QueueId) -> StoreChange,
) -> Vec<StoreChange> {
    let id = queue.id;
    let mut changes = vec![StoreChange::SaveQueue(queue)];
    if role == Some(QueueRole::OverlayAlerts) {
        changes.push(claim(id));
    }
    changes
}

pub(crate) fn absent<K: Ord + Clone>(
    local: impl IntoIterator<Item = K>,
    kept: &BTreeSet<K>,
) -> Vec<K> {
    local.into_iter().filter(|id| !kept.contains(id)).collect()
}
