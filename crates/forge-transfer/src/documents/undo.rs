use std::io;
use std::path::PathBuf;

use forge_storage::{
    DataProvider, GlobalEntry, GlobalsRepo, MediaBlobId, MediaReferrer, MediaReferrerKind,
    OverlayDefinition, OverlayId, ScriptRecord, ScriptRepo, StorageError, StoredClip,
};
use forge_types::{
    Action, ActionId, ClipId, Queue, QueueId, ScriptId, TriggerInstance, TriggerInstanceId,
};

pub(crate) enum Undo {
    Media(MediaBlobId),
    Queue {
        id: QueueId,
        previous: Option<Queue>,
    },
    Script {
        id: ScriptId,
        previous: Option<ScriptRecord>,
        archived: bool,
    },
    Global {
        name: String,
        previous: Option<GlobalEntry>,
        archived: bool,
    },
    Clip {
        id: ClipId,
        previous: Option<StoredClip>,
    },
    Referrer {
        referrer: MediaReferrer,
        previous: Option<MediaBlobId>,
    },
    Overlay {
        id: OverlayId,
        previous: Option<OverlayDefinition>,
    },
    OverlayDir(PathBuf),
    OverlayFile {
        path: PathBuf,
        previous: Option<Vec<u8>>,
    },
    Trigger {
        id: TriggerInstanceId,
        previous: Option<TriggerInstance>,
        archived: bool,
    },
    Action {
        id: ActionId,
        previous: Option<Action>,
        archived: bool,
    },
    Link {
        action: ActionId,
        trigger: TriggerInstanceId,
    },
}

#[derive(Default)]
pub(crate) struct UndoJournal {
    entries: Vec<Undo>,
}

impl UndoJournal {
    pub(crate) fn push(&mut self, undo: Undo) {
        self.entries.push(undo);
    }

    pub(crate) async fn roll_back(self, provider: &dyn DataProvider) -> Result<(), String> {
        let mut failures = Vec::new();
        for undo in self.entries.into_iter().rev() {
            if let Err(e) = revert(provider, undo).await {
                failures.push(e);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

pub(crate) async fn revert(provider: &dyn DataProvider, undo: Undo) -> Result<(), String> {
    let storage = |e: StorageError| e.to_string();
    let file = |e: io::Error| e.to_string();
    match undo {
        Undo::Media(id) => provider
            .media_repo()
            .delete(&id)
            .await
            .map(drop)
            .map_err(storage),
        Undo::Queue { id, previous } => {
            let repo = provider.queue_repo();
            match previous {
                Some(queue) => repo.save(&queue).await.map_err(storage),
                None => repo.delete(id).await.map(drop).map_err(storage),
            }
        }
        Undo::Script {
            id,
            previous,
            archived,
        } => match previous {
            Some(record) => {
                ScriptRepo::save(provider, record).await.map_err(storage)?;
                if archived {
                    provider.archive_script(id).await.map_err(storage)?;
                }
                Ok(())
            }
            None => ScriptRepo::delete(provider, id)
                .await
                .map(drop)
                .map_err(storage),
        },
        Undo::Global {
            name,
            previous,
            archived,
        } => match previous {
            Some(entry) => {
                GlobalsRepo::set(provider, &name, entry.value, entry.persisted)
                    .await
                    .map_err(storage)?;
                if archived {
                    GlobalsRepo::archive(provider, &name)
                        .await
                        .map_err(storage)?;
                }
                Ok(())
            }
            None => GlobalsRepo::delete(provider, &name)
                .await
                .map(drop)
                .map_err(storage),
        },
        Undo::Clip { id, previous } => {
            let repo = provider.soundboard_clips_repo();
            match previous {
                Some(clip) => repo.save(&clip).await.map_err(storage),
                None => repo.delete(id).await.map(drop).map_err(storage),
            }
        }
        Undo::Referrer { referrer, previous } => {
            let repo = provider.media_repo();
            match previous {
                Some(blob) => repo.retain(&referrer, &blob).await.map_err(storage),
                None => repo.release(&referrer).await.map(drop).map_err(storage),
            }
        }
        Undo::Overlay { id, previous } => {
            let repo = provider.overlay_repo();
            match previous {
                Some(definition) => repo.restore(&definition).await.map_err(storage),
                None => {
                    provider
                        .media_repo()
                        .release_all(MediaReferrerKind::Overlay, id.as_str())
                        .await
                        .map_err(storage)?;
                    repo.delete(&id).await.map(drop).map_err(storage)
                }
            }
        }
        Undo::OverlayDir(path) => match tokio::fs::remove_dir_all(&path).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(file(e)),
            _ => Ok(()),
        },
        Undo::OverlayFile { path, previous } => match previous {
            Some(bytes) => tokio::fs::write(&path, bytes).await.map_err(file),
            None => match tokio::fs::remove_file(&path).await {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(file(e)),
                _ => Ok(()),
            },
        },
        Undo::Trigger {
            id,
            previous,
            archived,
        } => {
            let repo = provider.trigger_instance_repo();
            match previous {
                Some(trigger) => {
                    repo.save(&trigger).await.map_err(storage)?;
                    if archived {
                        repo.archive(id).await.map_err(storage)?;
                    }
                    Ok(())
                }
                None => repo.delete(id).await.map(drop).map_err(storage),
            }
        }
        Undo::Action {
            id,
            previous,
            archived,
        } => {
            let repo = provider.action_repo();
            match previous {
                Some(action) => {
                    repo.save(&action).await.map_err(storage)?;
                    if archived {
                        repo.archive(id).await.map_err(storage)?;
                    }
                    Ok(())
                }
                None => repo.delete(id).await.map(drop).map_err(storage),
            }
        }
        Undo::Link { action, trigger } => provider
            .trigger_instance_repo()
            .unlink_action(action, trigger)
            .await
            .map(drop)
            .map_err(storage),
    }
}
