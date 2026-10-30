use std::collections::BTreeSet;

use forge_storage::{MediaReferrerKind, OverlayId, StoreChange, clip_source_referrer};
use forge_types::{ActionId, ClipId, QueueId, ScriptId, TriggerInstanceId};

use crate::error::TransferError;
use crate::setup::import_apply::{SetupImport, SetupLivePart};

impl SetupImport<'_, '_> {
    fn setup_action_ids(&self) -> BTreeSet<ActionId> {
        let configuration = &self.staged.documents().configuration;
        configuration
            .actions
            .iter()
            .map(|action| action.id)
            .collect()
    }

    pub(super) fn setup_trigger_ids(&self) -> BTreeSet<TriggerInstanceId> {
        let configuration = &self.staged.documents().configuration;
        configuration
            .triggers
            .iter()
            .map(|trigger| trigger.id)
            .collect()
    }

    pub(super) fn setup_queue_ids(&self) -> BTreeSet<QueueId> {
        let configuration = &self.staged.documents().configuration;
        configuration.queues.iter().map(|queue| queue.id).collect()
    }

    pub(super) fn unarchive_local(&mut self) {
        let configuration = &self.inventory.configuration;
        let triggers = configuration.archived_triggers.keys().copied();
        let actions = configuration.archived_actions.keys().copied();
        self.changes
            .extend(triggers.map(StoreChange::RestoreTrigger));
        self.changes.extend(actions.map(StoreChange::RestoreAction));
    }

    fn local_action_ids(&self) -> BTreeSet<ActionId> {
        let configuration = &self.inventory.configuration;
        configuration
            .actions
            .keys()
            .chain(configuration.archived_actions.keys())
            .copied()
            .collect()
    }

    pub(super) async fn unlink_local(&mut self) -> Result<(), TransferError> {
        let repo = self.provider.trigger_instance_repo();
        let mut links_change = !self.inventory.configuration.archived_triggers.is_empty();
        for action in self.local_action_ids() {
            links_change |= !repo.list_for_action(action).await?.is_empty();
            self.changes.push(StoreChange::UnlinkAllTriggers(action));
        }
        if links_change {
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    pub(super) fn remove_absent_triggers(&mut self) {
        let kept = self.setup_trigger_ids();
        let configuration = &self.inventory.configuration;
        let absent: Vec<_> = configuration
            .triggers
            .keys()
            .chain(configuration.archived_triggers.keys())
            .filter(|id| !kept.contains(id))
            .copied()
            .collect();
        for id in absent {
            self.changes.push(StoreChange::DeleteTrigger(id));
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
    }

    pub(super) fn remove_absent_scripts(&mut self) {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<ScriptId> = configuration
            .scripts
            .iter()
            .map(|script| script.id)
            .collect();
        let local = &self.inventory.configuration;
        let absent: Vec<ScriptId> = local
            .scripts
            .values()
            .chain(local.archived_scripts.values())
            .map(|script| script.id)
            .filter(|id| !kept.contains(id))
            .collect();
        for id in absent {
            self.changes.push(StoreChange::DeleteScript(id));
            self.refresh.insert(SetupLivePart::Scripts);
        }
    }

    pub(super) fn remove_absent_clips(&mut self) {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<ClipId> = configuration.clips.iter().map(|clip| clip.id).collect();
        let absent: Vec<ClipId> = self
            .inventory
            .configuration
            .clips
            .keys()
            .filter(|id| !kept.contains(id))
            .copied()
            .collect();
        for id in absent {
            if self.inventory.configuration.clip_media.contains_key(&id) {
                self.changes
                    .push(StoreChange::ReleaseMedia(clip_source_referrer(id)));
            }
            self.changes.push(StoreChange::DeleteClip(id));
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
    }

    pub(super) fn remove_absent_globals(&mut self) {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<&str> = configuration
            .globals
            .iter()
            .map(|global| global.name.as_str())
            .collect();
        let local = &self.inventory.configuration;
        let absent = local
            .globals
            .values()
            .filter(|global| global.persisted)
            .chain(local.archived_globals.values())
            .filter(|global| !kept.contains(global.name.as_str()))
            .map(|global| StoreChange::DeleteGlobal(global.name.clone()));
        self.changes.extend(absent);
    }

    pub(super) fn remove_absent_voice_aliases(&mut self) {
        let voices = &self.staged.documents().voices;
        let kept: BTreeSet<&str> = voices
            .aliases
            .iter()
            .map(|alias| alias.id.0.as_str())
            .collect();
        let absent: Vec<_> = self
            .inventory
            .voice_aliases
            .iter()
            .filter(|alias| !kept.contains(alias.id.0.as_str()))
            .map(|alias| alias.id.clone())
            .collect();
        for id in absent {
            self.changes.push(StoreChange::DeleteVoiceAlias(id));
            self.refresh.insert(SetupLivePart::Tts);
        }
    }

    pub(super) fn remove_absent_actions(&mut self) {
        let kept = self.setup_action_ids();
        let configuration = &self.inventory.configuration;
        let absent = configuration
            .actions
            .keys()
            .chain(configuration.archived_actions.keys())
            .filter(|id| !kept.contains(id))
            .map(|id| StoreChange::DeleteAction(*id));
        self.changes.extend(absent);
    }

    pub(super) fn remove_absent_queues(&mut self) {
        let kept = self.setup_queue_ids();
        let absent: Vec<QueueId> = self
            .inventory
            .configuration
            .queues
            .values()
            .filter(|queue| !queue.is_default() && !kept.contains(&queue.id))
            .map(|queue| queue.id)
            .collect();
        for id in absent {
            self.changes.push(StoreChange::DeleteQueue(id));
            self.queues.removed.push(id);
            self.refresh.insert(SetupLivePart::Queues);
        }
    }

    pub(super) fn remove_absent_overlays(&mut self) {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<OverlayId> = configuration
            .overlays
            .iter()
            .map(|overlay| OverlayId::new(overlay.id.as_str()))
            .collect();
        let absent: Vec<OverlayId> = self
            .inventory
            .configuration
            .overlays
            .keys()
            .filter(|id| !kept.contains(*id))
            .cloned()
            .collect();
        for id in absent {
            self.changes.push(StoreChange::ReleaseAllMedia {
                kind: MediaReferrerKind::Overlay,
                referrer_id: id.as_str().to_owned(),
            });
            self.changes.push(StoreChange::DeleteOverlay(id.clone()));
            self.removed_overlays.push(id);
            self.refresh.insert(SetupLivePart::Overlays);
        }
    }
}
