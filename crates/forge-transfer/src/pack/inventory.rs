use std::collections::BTreeMap;

use forge_storage::{
    DataProvider, GlobalEntry, GlobalsRepo, MediaBlob, MediaBlobId, OverlayDefinition, OverlayId,
    ScriptRecord, ScriptRepo, StoredClip, clip_source_referrer,
};
use forge_types::{Action, ActionId, ClipId, Queue, QueueId, TriggerInstance, TriggerInstanceId};

use crate::error::TransferError;

#[derive(Debug, Clone, Default)]
pub struct PackInventory {
    pub(crate) actions: BTreeMap<ActionId, Action>,
    pub(crate) action_triggers: BTreeMap<ActionId, Vec<TriggerInstanceId>>,
    pub(crate) triggers: BTreeMap<TriggerInstanceId, TriggerInstance>,
    pub(crate) queues: BTreeMap<QueueId, Queue>,
    pub(crate) scripts: BTreeMap<String, ScriptRecord>,
    pub(crate) globals: BTreeMap<String, GlobalEntry>,
    pub(crate) clips: BTreeMap<ClipId, StoredClip>,
    pub(crate) clip_media: BTreeMap<ClipId, MediaBlobId>,
    pub(crate) overlays: BTreeMap<OverlayId, OverlayDefinition>,
    pub(crate) media: BTreeMap<MediaBlobId, MediaBlob>,
}

impl PackInventory {
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }
}

pub async fn load_pack_inventory(
    provider: &dyn DataProvider,
) -> Result<PackInventory, TransferError> {
    let actions = provider.action_repo().list().await?;
    let trigger_repo = provider.trigger_instance_repo();
    let mut action_triggers = BTreeMap::new();
    for action in &actions {
        let linked = trigger_repo.list_for_action(action.id).await?;
        action_triggers.insert(action.id, linked.iter().map(|trigger| trigger.id).collect());
    }
    let media_repo = provider.media_repo();
    let clips = provider.soundboard_clips_repo().list().await?;
    let mut clip_media = BTreeMap::new();
    for clip in &clips {
        if let Some(blob) = media_repo.blob_of(&clip_source_referrer(clip.id)).await? {
            clip_media.insert(clip.id, blob);
        }
    }
    Ok(PackInventory {
        actions: actions
            .into_iter()
            .map(|action| (action.id, action))
            .collect(),
        action_triggers,
        triggers: keyed(trigger_repo.list_all().await?, |trigger| trigger.id),
        queues: keyed(provider.queue_repo().list().await?, |queue| queue.id),
        scripts: keyed(ScriptRepo::list(provider).await?, |script| {
            script.name.clone()
        }),
        globals: keyed(GlobalsRepo::list(provider).await?, |global| {
            global.name.clone()
        }),
        clips: keyed(clips, |clip| clip.id),
        clip_media,
        overlays: keyed(provider.overlay_repo().list().await?, |overlay| {
            overlay.id.clone()
        }),
        media: keyed(media_repo.list().await?, |blob| blob.id.clone()),
    })
}

fn keyed<K: Ord, V>(values: Vec<V>, key: impl Fn(&V) -> K) -> BTreeMap<K, V> {
    values
        .into_iter()
        .map(|value| (key(&value), value))
        .collect()
}
