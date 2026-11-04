use super::*;
use crate::shared::repo_failure::RepoFailure;

struct BulkTally {
    waiting: usize,
    archived: Vec<ActionId>,
    failure: Option<RepoFailure>,
}

pub(super) struct SettledBulk {
    archived: Vec<ActionId>,
    failure: Option<RepoFailure>,
}

#[derive(Default)]
pub(super) struct BulkArchives {
    issued: u64,
    open: HashMap<u64, BulkTally>,
}

impl BulkArchives {
    fn open(&mut self, count: usize) -> u64 {
        self.issued += 1;
        self.open.insert(
            self.issued,
            BulkTally {
                waiting: count,
                archived: Vec::with_capacity(count),
                failure: None,
            },
        );
        self.issued
    }

    fn settle(
        &mut self,
        batch: u64,
        id: ActionId,
        archived: Result<(), RepoFailure>,
    ) -> Option<SettledBulk> {
        let tally = self.open.get_mut(&batch)?;
        tally.waiting = tally.waiting.saturating_sub(1);
        match archived {
            Ok(()) => tally.archived.push(id),
            Err(failure) => {
                tally.failure.get_or_insert(failure);
            }
        }
        if tally.waiting > 0 {
            return None;
        }
        let tally = self.open.remove(&batch)?;
        Some(SettledBulk {
            archived: tally.archived,
            failure: tally.failure,
        })
    }
}

impl ActionEditor {
    pub(in crate::automation::actions_screen) fn archive_in_bulk(
        &mut self,
        ids: Vec<ActionId>,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        let batch = self.bulk_archives.open(ids.len());
        for id in ids {
            self.write_action(id, ActionWrite::ArchiveInBulk { batch }, cx);
        }
    }

    pub(super) fn settle_bulk_archive(
        &mut self,
        batch: u64,
        id: ActionId,
        archived: Result<(), RepoFailure>,
        cx: &mut Context<Self>,
    ) {
        let Some(settled) = self.bulk_archives.settle(batch, id, archived) else {
            return;
        };
        if let Some(failure) = &settled.failure {
            on_repo_error(failure, cx);
        }
        if !settled.archived.is_empty() {
            self.raise_bulk_undo_toast(settled.archived, cx);
        }
    }

    fn raise_bulk_undo_toast(&self, ids: Vec<ActionId>, cx: &mut Context<Self>) {
        let view = cx.entity();
        let count = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        let message = tr!("bulk_deleted_actions_toast", count = count);
        cx.push_undo_toast(message, move |_window, app: &mut App| {
            view.update(app, |this, cx| {
                for id in &ids {
                    this.write_action(*id, ActionWrite::Restore, cx);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use forge_storage::{MockActionRepo, MockMediaRepo, StorageError};
    use gpui::TestAppContext;

    use super::super::tests::{screen_parts, view_with};
    use super::*;
    use crate::test_support::{action_running, error_toasts, press_undo, runtime};

    #[test]
    fn two_bulk_deletes_in_flight_settle_on_their_own_counts() {
        let (first, second, third) = (ActionId::new(), ActionId::new(), ActionId::new());
        let mut archives = BulkArchives::default();
        let older = archives.open(2);
        let newer = archives.open(1);

        let older_after_one = archives.settle(older, first, Ok(())).is_some();
        let newer_done = archives.settle(newer, second, Ok(())).map(|s| s.archived);
        let older_done = archives.settle(older, third, Ok(())).map(|s| s.archived);

        assert_eq!(
            (older_after_one, newer_done, older_done),
            (false, Some(vec![second]), Some(vec![first, third]))
        );
    }

    #[test]
    fn a_settle_for_a_batch_that_already_finished_reports_nothing() {
        let id = ActionId::new();
        let mut archives = BulkArchives::default();
        let batch = archives.open(1);
        let finished = archives.settle(batch, id, Ok(())).is_some();

        let again = archives.settle(batch, id, Ok(())).is_some();

        assert_eq!((finished, again), (true, false));
    }

    struct Stored {
        live: Vec<Action>,
        archived: Vec<Action>,
        restored: Vec<ActionId>,
    }

    struct Rig {
        editor: Entity<ActionEditor>,
        _host: Entity<ScreenActionsView>,
        stored: Arc<Mutex<Stored>>,
        ids: Vec<ActionId>,
        rt: tokio::runtime::Runtime,
    }

    fn open_failing_archives_of(cx: &mut TestAppContext, failing: &[usize]) -> Rig {
        crate::app::i18n::install_language(forge_storage::Language::En);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let rt = runtime();
        let actions: Vec<Action> = ["Alpha", "Bravo", "Charlie"]
            .into_iter()
            .map(|name| action_running(name, Vec::new()))
            .collect();
        let ids: Vec<ActionId> = actions.iter().map(|action| action.id).collect();
        let refused: HashSet<ActionId> = failing.iter().map(|&at| ids[at]).collect();
        let stored = Arc::new(Mutex::new(Stored {
            live: actions,
            archived: Vec::new(),
            restored: Vec::new(),
        }));

        let mut repo = MockActionRepo::new();
        let listing = Arc::clone(&stored);
        repo.expect_list()
            .returning(move || Ok(listing.lock().unwrap().live.clone()));
        let reading = Arc::clone(&stored);
        repo.expect_get().returning(move |id| {
            let stored = reading.lock().unwrap();
            Ok(stored.live.iter().find(|action| action.id == id).cloned())
        });
        repo.expect_telemetry()
            .returning(|_| Ok(forge_storage::ActionTelemetry::default()));
        let shelving = Arc::clone(&stored);
        repo.expect_archive().returning(move |id| {
            if refused.contains(&id) {
                return Err(StorageError::Connection {
                    reason: "database is locked".to_owned(),
                });
            }
            let mut stored = shelving.lock().unwrap();
            let at = stored
                .live
                .iter()
                .position(|action| action.id == id)
                .unwrap();
            let action = stored.live.remove(at);
            stored.archived.push(action);
            Ok(true)
        });
        let restoring = Arc::clone(&stored);
        repo.expect_restore().returning(move |id| {
            let mut stored = restoring.lock().unwrap();
            stored.restored.push(id);
            let at = stored.archived.iter().position(|action| action.id == id);
            let action = stored.archived.remove(at.unwrap());
            stored.live.push(action);
            Ok(true)
        });

        let host = view_with(
            cx,
            &rt,
            Vec::new(),
            MockMediaRepo::new(),
            SubActionRegistry::new(),
            Arc::new(repo),
        );
        let (editor, _tree) = screen_parts(cx, &host);
        let rig = Rig {
            editor,
            _host: host,
            stored,
            ids,
            rt,
        };
        rig.settle(cx);
        rig
    }

    impl Rig {
        fn settle(&self, cx: &mut TestAppContext) {
            crate::test_support::settle_runtime_work(cx, &self.rt);
        }

        fn delete_all(&self, cx: &mut TestAppContext) {
            let ids = self.ids.clone();
            self.editor
                .update(cx, |editor, cx| editor.archive_in_bulk(ids, cx));
            self.settle(cx);
        }
    }

    fn undo_toasts(cx: &mut TestAppContext) -> Vec<String> {
        cx.update(|cx| {
            cx.global::<forge_components::Toasts>()
                .items()
                .iter()
                .filter(|toast| toast.kind == forge_components::ToastKind::Undo)
                .map(|toast| toast.message.to_string())
                .collect()
        })
    }

    #[gpui::test]
    fn a_partly_failed_bulk_delete_shows_one_error_and_offers_undo_for_the_archived_ones(
        cx: &mut TestAppContext,
    ) {
        let rig = open_failing_archives_of(cx, &[1]);

        rig.delete_all(cx);

        assert_eq!(
            (cx.update(|cx| error_toasts(cx)), undo_toasts(cx)),
            (
                1,
                vec![tr!("bulk_deleted_actions_toast", count = 2).to_string()]
            )
        );
    }

    #[gpui::test]
    fn undo_of_a_partly_failed_bulk_delete_restores_exactly_the_archived_actions(
        cx: &mut TestAppContext,
    ) {
        let rig = open_failing_archives_of(cx, &[1]);
        rig.delete_all(cx);

        press_undo(cx);
        rig.settle(cx);

        let restored: HashSet<ActionId> = rig
            .stored
            .lock()
            .unwrap()
            .restored
            .iter()
            .copied()
            .collect();
        assert_eq!(restored, HashSet::from([rig.ids[0], rig.ids[2]]));
    }

    #[gpui::test]
    fn a_bulk_delete_where_every_archive_fails_shows_one_error_and_no_undo(
        cx: &mut TestAppContext,
    ) {
        let rig = open_failing_archives_of(cx, &[0, 1, 2]);

        rig.delete_all(cx);

        assert_eq!(
            (cx.update(|cx| error_toasts(cx)), undo_toasts(cx)),
            (1, Vec::<String>::new())
        );
    }
}
