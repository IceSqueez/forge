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
