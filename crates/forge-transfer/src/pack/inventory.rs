use std::collections::BTreeMap;

use forge_storage::{
    DataProvider, GlobalEntry, GlobalsRepo, MediaBlob, MediaBlobId, OverlayDefinition, OverlayId,
    ScriptRecord, ScriptRepo, StoredClip, clip_source_referrer,
};
use forge_types::{
    Action, ActionId, ClipId, Queue, QueueId, ScriptId, TriggerInstance, TriggerInstanceId,
};

use crate::error::TransferError;

#[derive(Debug, Clone, Default)]
pub struct PackInventory {
    pub(crate) actions: BTreeMap<ActionId, Action>,
    pub(crate) archived_actions: BTreeMap<ActionId, Action>,
    pub(crate) action_triggers: BTreeMap<ActionId, Vec<TriggerInstanceId>>,
    pub(crate) triggers: BTreeMap<TriggerInstanceId, TriggerInstance>,
    pub(crate) archived_triggers: BTreeMap<TriggerInstanceId, TriggerInstance>,
    pub(crate) queues: BTreeMap<QueueId, Queue>,
    pub(crate) scripts: BTreeMap<String, ScriptRecord>,
    pub(crate) archived_scripts: BTreeMap<String, ScriptRecord>,
    pub(crate) globals: BTreeMap<String, GlobalEntry>,
    pub(crate) archived_globals: BTreeMap<String, GlobalEntry>,
    pub(crate) clips: BTreeMap<ClipId, StoredClip>,
    pub(crate) clip_media: BTreeMap<ClipId, MediaBlobId>,
    pub(crate) overlays: BTreeMap<OverlayId, OverlayDefinition>,
    pub(crate) media: BTreeMap<MediaBlobId, MediaBlob>,
}

impl PackInventory {
    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    pub(crate) fn stored_action(&self, id: &ActionId) -> Option<Stored<Action>> {
        stored(&self.actions, &self.archived_actions, id)
    }

    pub(crate) fn stored_trigger(&self, id: &TriggerInstanceId) -> Option<Stored<TriggerInstance>> {
        stored(&self.triggers, &self.archived_triggers, id)
    }

    pub(crate) fn stored_global(&self, name: &str) -> Option<Stored<GlobalEntry>> {
        stored(&self.globals, &self.archived_globals, name)
    }

    pub(crate) fn stored_script_named(&self, name: &str) -> Option<Stored<ScriptRecord>> {
        stored(&self.scripts, &self.archived_scripts, name)
    }

    pub(crate) fn stored_script(&self, id: ScriptId) -> Option<Stored<ScriptRecord>> {
        let name = self
            .scripts
            .values()
            .chain(self.archived_scripts.values())
            .find(|script| script.id == id)
            .map(|script| script.name.clone())?;
        self.stored_script_named(&name)
    }

    pub(crate) fn script_names(&self) -> impl Iterator<Item = &str> {
        self.scripts
            .keys()
            .chain(self.archived_scripts.keys())
            .map(String::as_str)
    }
}

pub async fn load_pack_inventory(
    provider: &dyn DataProvider,
) -> Result<PackInventory, TransferError> {
    let action_repo = provider.action_repo();
    let actions = action_repo.list().await?;
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
        archived_actions: keyed(action_repo.list_archived().await?, |action| action.id),
        action_triggers,
        triggers: keyed(trigger_repo.list_all().await?, |trigger| trigger.id),
        archived_triggers: keyed(trigger_repo.list_archived().await?, |trigger| trigger.id),
        queues: keyed(provider.queue_repo().list().await?, |queue| queue.id),
        scripts: keyed(ScriptRepo::list(provider).await?, |script| {
            script.name.clone()
        }),
        archived_scripts: keyed(provider.list_archived_scripts().await?, |script| {
            script.name.clone()
        }),
        globals: keyed(GlobalsRepo::list(provider).await?, |global| {
            global.name.clone()
        }),
        archived_globals: keyed(GlobalsRepo::list_archived(provider).await?, |global| {
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

#[derive(Debug, Clone)]
pub(crate) struct Stored<T> {
    pub(crate) row: T,
    pub(crate) archived: bool,
}

fn stored<K, Q, T>(live: &BTreeMap<K, T>, archived: &BTreeMap<K, T>, key: &Q) -> Option<Stored<T>>
where
    K: Ord + std::borrow::Borrow<Q>,
    Q: Ord + ?Sized,
    T: Clone,
{
    live.get(key)
        .map(|row| Stored {
            row: row.clone(),
            archived: false,
        })
        .or_else(|| {
            archived.get(key).map(|row| Stored {
                row: row.clone(),
                archived: true,
            })
        })
}

fn keyed<K: Ord, V>(values: Vec<V>, key: impl Fn(&V) -> K) -> BTreeMap<K, V> {
    values
        .into_iter()
        .map(|value| (key(&value), value))
        .collect()
}
