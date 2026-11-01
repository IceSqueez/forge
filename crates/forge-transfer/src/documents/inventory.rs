use std::collections::BTreeMap;

use forge_storage::{
    CLIP_SOURCE_SLOT, DataProvider, GlobalEntry, MediaBlob, MediaBlobId, MediaReferrerKind,
    OVERLAY_ALERT_QUEUE_NAME, OverlayDefinition, OverlayId, QueueRole, ScriptRecord, StoredClip,
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
    pub(crate) overlay_alert_queue: Option<QueueId>,
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
    pub(crate) fn queue_role(&self, id: QueueId) -> Option<QueueRole> {
        (self.overlay_alert_queue == Some(id)).then_some(QueueRole::OverlayAlerts)
    }

    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.actions.values()
    }

    pub(crate) fn stored_action(&self, id: &ActionId) -> Option<Stored<Action>> {
        stored(&self.actions, &self.archived_actions, id)
    }

    pub(crate) fn stored_trigger(&self, id: &TriggerInstanceId) -> Option<Stored<TriggerInstance>> {
        stored(&self.triggers, &self.archived_triggers, id)
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
    let mut links = trigger_repo.instances_by_action().await?;
    let action_triggers = actions
        .iter()
        .map(|action| {
            let linked = links.remove(&action.id).unwrap_or_default();
            (action.id, linked.iter().map(|trigger| trigger.id).collect())
        })
        .collect();
    let media_repo = provider.media_repo();
    let clips = provider.soundboard_clips_repo().list().await?;
    let mut clip_blobs = media_repo
        .blobs_by_referrer_id(MediaReferrerKind::SoundboardClip, CLIP_SOURCE_SLOT)
        .await?;
    let clip_media = clips
        .iter()
        .filter_map(|clip| {
            let blob = clip_blobs.remove(&clip.id.to_string())?;
            Some((clip.id, blob))
        })
        .collect();
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
        overlay_alert_queue: provider
            .queue_repo()
            .overlay_alert_queue(OVERLAY_ALERT_QUEUE_NAME)
            .await?
            .map(|queue| queue.id),
        scripts: keyed(provider.script_repo().list().await?, |script| {
            script.name.clone()
        }),
        archived_scripts: keyed(
            provider.script_repo().list_archived_scripts().await?,
            |script| script.name.clone(),
        ),
        globals: keyed(provider.globals_repo().list().await?, |global| {
            global.name.clone()
        }),
        archived_globals: keyed(provider.globals_repo().list_archived().await?, |global| {
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
