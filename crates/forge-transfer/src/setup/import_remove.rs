use std::collections::BTreeSet;

use forge_storage::{GlobalsRepo, MediaReferrerKind, OverlayId, ScriptRepo, clip_source_referrer};
use forge_types::{ActionId, ClipId, QueueId, ScriptId, TriggerInstanceId};

use crate::error::TransferError;
use crate::pack::import_undo::Undo;
use crate::setup::import_apply::{SetupImport, SetupLivePart};
use crate::setup::import_undo::SetupUndo;

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

    pub(super) async fn unarchive_local(&mut self) -> Result<(), TransferError> {
        let triggers = self.provider.trigger_instance_repo();
        let archived: Vec<_> = self
            .inventory
            .configuration
            .archived_triggers
            .values()
            .cloned()
            .collect();
        for trigger in archived {
            triggers.restore(trigger.id).await?;
            self.journal.push_configuration(Undo::Trigger {
                id: trigger.id,
                previous: Some(trigger),
                archived: true,
            });
        }
        let actions = self.provider.action_repo();
        let archived: Vec<_> = self
            .inventory
            .configuration
            .archived_actions
            .values()
            .cloned()
            .collect();
        for action in archived {
            actions.restore(action.id).await?;
            self.journal.push_configuration(Undo::Action {
                id: action.id,
                previous: Some(action),
                archived: true,
            });
        }
        Ok(())
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
        for action in self.local_action_ids() {
            let linked = repo.list_for_action(action).await?;
            for (position, trigger) in linked.iter().enumerate() {
                repo.unlink_action(action, trigger.id).await?;
                self.journal.push(SetupUndo::Relink {
                    action,
                    trigger: trigger.id,
                    position: i64::try_from(position).unwrap_or(i64::MAX),
                });
                self.refresh.insert(SetupLivePart::Hotkeys);
            }
        }
        Ok(())
    }

    pub(super) async fn remove_absent_triggers(&mut self) -> Result<(), TransferError> {
        let kept = self.setup_trigger_ids();
        let configuration = &self.inventory.configuration;
        let absent: Vec<_> = configuration
            .triggers
            .values()
            .chain(configuration.archived_triggers.values())
            .filter(|trigger| !kept.contains(&trigger.id))
            .cloned()
            .collect();
        let repo = self.provider.trigger_instance_repo();
        for trigger in absent {
            repo.delete(trigger.id).await?;
            self.journal.push_configuration(Undo::Trigger {
                id: trigger.id,
                previous: Some(trigger),
                archived: false,
            });
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    pub(super) async fn remove_absent_scripts(&mut self) -> Result<(), TransferError> {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<ScriptId> = configuration
            .scripts
            .iter()
            .map(|script| script.id)
            .collect();
        let absent: Vec<_> = self
            .inventory
            .configuration
            .scripts
            .values()
            .filter(|script| !kept.contains(&script.id))
            .cloned()
            .collect();
        for script in absent {
            ScriptRepo::delete(self.provider, script.id).await?;
            self.journal.push_configuration(Undo::Script {
                id: script.id,
                previous: Some(script),
            });
            self.refresh.insert(SetupLivePart::Scripts);
        }
        Ok(())
    }

    pub(super) async fn remove_absent_clips(&mut self) -> Result<(), TransferError> {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<ClipId> = configuration.clips.iter().map(|clip| clip.id).collect();
        let absent: Vec<_> = self
            .inventory
            .configuration
            .clips
            .values()
            .filter(|clip| !kept.contains(&clip.id))
            .cloned()
            .collect();
        let media = self.provider.media_repo();
        let clips = self.provider.soundboard_clips_repo();
        for clip in absent {
            let referrer = clip_source_referrer(clip.id);
            if let Some(blob) = self.inventory.configuration.clip_media.get(&clip.id) {
                media.release(&referrer).await?;
                self.journal.push_configuration(Undo::Referrer {
                    referrer,
                    previous: Some(blob.clone()),
                });
            }
            clips.delete(clip.id).await?;
            self.journal.push_configuration(Undo::Clip {
                id: clip.id,
                previous: Some(clip),
            });
            self.refresh.insert(SetupLivePart::Hotkeys);
        }
        Ok(())
    }

    pub(super) async fn remove_absent_globals(&mut self) -> Result<(), TransferError> {
        let configuration = &self.staged.documents().configuration;
        let kept: BTreeSet<&str> = configuration
            .globals
            .iter()
            .map(|global| global.name.as_str())
            .collect();
        let local = &self.inventory.configuration;
        let live = local
            .globals
            .values()
            .filter(|global| global.persisted)
            .map(|global| (global.clone(), false));
        let archived = local
            .archived_globals
            .values()
            .map(|global| (global.clone(), true));
        let absent: Vec<_> = live
            .chain(archived)
            .filter(|(global, _)| !kept.contains(global.name.as_str()))
            .collect();
        for (global, archived) in absent {
            GlobalsRepo::delete(self.provider, &global.name).await?;
            self.journal.push_configuration(Undo::Global {
                name: global.name.clone(),
                previous: Some(global),
                archived,
            });
        }
        Ok(())
    }

    pub(super) async fn remove_absent_voice_aliases(&mut self) -> Result<(), TransferError> {
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
            .cloned()
            .collect();
        let repo = self.provider.voice_alias_repo();
        for alias in absent {
            repo.delete(&alias.id).await?;
            self.journal.push(SetupUndo::VoiceAlias {
                id: alias.id.clone(),
                previous: Some(alias),
            });
            self.refresh.insert(SetupLivePart::Tts);
        }
        Ok(())
    }

    pub(super) async fn remove_absent_actions(&mut self) -> Result<(), TransferError> {
        let kept = self.setup_action_ids();
        let configuration = &self.inventory.configuration;
        let absent: Vec<_> = configuration
            .actions
            .values()
            .chain(configuration.archived_actions.values())
            .filter(|action| !kept.contains(&action.id))
            .cloned()
            .collect();
        let repo = self.provider.action_repo();
        for action in absent {
            repo.delete(action.id).await?;
            self.journal.push_configuration(Undo::Action {
                id: action.id,
                previous: Some(action),
                archived: false,
            });
        }
        Ok(())
    }

    pub(super) async fn remove_absent_queues(&mut self) -> Result<(), TransferError> {
        let kept = self.setup_queue_ids();
        let absent: Vec<_> = self
            .inventory
            .configuration
            .queues
            .values()
            .filter(|queue| !kept.contains(&queue.id))
            .cloned()
            .collect();
        let repo = self.provider.queue_repo();
        for queue in absent {
            repo.delete(queue.id).await?;
            self.queues.removed.push(queue.id);
            self.journal.push_configuration(Undo::Queue {
                id: queue.id,
                previous: Some(queue),
            });
            self.refresh.insert(SetupLivePart::Queues);
        }
        Ok(())
    }

    pub(super) async fn remove_absent_overlays(&mut self) -> Result<(), TransferError> {
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
        let media = self.provider.media_repo();
        let overlays = self.provider.overlay_repo();
        for id in absent {
            self.journal.push(SetupUndo::RemovedOverlay(id.clone()));
            media
                .release_all(MediaReferrerKind::Overlay, id.as_str())
                .await?;
            overlays.delete(&id).await?;
            self.removed_overlays.push(id);
            self.refresh.insert(SetupLivePart::Overlays);
        }
        Ok(())
    }
}
