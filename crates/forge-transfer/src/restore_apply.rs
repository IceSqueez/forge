use std::path::Path;

use forge_storage::DATABASE_FILE_NAME;
use forge_storage::paths::credentials_key_path_in;

use crate::durable_fs::{
    copy_atomically, exists, remove_dir_all_if_present, remove_file_if_present, sync_dir,
    sync_parent,
};
use crate::error::TransferError;
use crate::layout::ARCHIVE_ROOT;
use crate::naming::fresh_pre_restore_dir;
use crate::restore_journal::{RestoreJournal, RestorePhase, backup_scope_entries};
use crate::restore_marker::{RestoreMarker, update_in_flight};
use crate::restore_outcome::{RestoreDeferral, RestoreOutcome, RestoreRollback};
use crate::stage::restore_staging_root;

pub fn apply_pending_restore(data_dir: &Path) -> Result<RestoreOutcome, TransferError> {
    if let Some(journal) = RestoreJournal::read(data_dir)? {
        return resume(data_dir, journal);
    }
    let marker = match RestoreMarker::read(data_dir) {
        Ok(Some(marker)) => marker,
        Ok(None) => {
            let _ = remove_dir_all_if_present(&restore_staging_root(data_dir));
            return Ok(RestoreOutcome::NonePending);
        }
        Err(TransferError::MalformedRecord { .. }) => {
            return abandon(data_dir, RestoreRollback::MarkerUnreadable);
        }
        Err(e) => return Err(e),
    };
    if update_in_flight(data_dir)? {
        return Ok(RestoreOutcome::Deferred {
            reason: RestoreDeferral::UpdateInProgress,
        });
    }
    let Ok(staging_dir) = marker.staging_dir(data_dir) else {
        return abandon(data_dir, RestoreRollback::MarkerUnreadable);
    };
    if !staging_dir
        .join(ARCHIVE_ROOT)
        .join(DATABASE_FILE_NAME)
        .is_file()
    {
        return abandon(data_dir, RestoreRollback::StagingMissing);
    }
    let journal = RestoreJournal::begin(
        staging_dir,
        fresh_pre_restore_dir(data_dir),
        credentials_key_path_in(data_dir),
    )?;
    journal.write(data_dir)?;
    run_forward(data_dir, journal)
}

fn resume(data_dir: &Path, journal: RestoreJournal) -> Result<RestoreOutcome, TransferError> {
    match journal.phase.clone() {
        RestorePhase::SnapshottingCurrent => {
            start_rollback(data_dir, journal, RestoreRollback::Interrupted)
        }
        RestorePhase::PlacingStaged => place_then_commit(data_dir, journal),
        RestorePhase::Committed => commit(data_dir, journal),
        RestorePhase::RollingBack { reason } => roll_back(data_dir, &journal, reason),
    }
}

fn run_forward(
    data_dir: &Path,
    mut journal: RestoreJournal,
) -> Result<RestoreOutcome, TransferError> {
    if let Err(e) = snapshot_current(data_dir, &journal) {
        return start_rollback(data_dir, journal, failed(&e));
    }
    journal.advance(data_dir, RestorePhase::PlacingStaged)?;
    place_then_commit(data_dir, journal)
}

fn place_then_commit(
    data_dir: &Path,
    journal: RestoreJournal,
) -> Result<RestoreOutcome, TransferError> {
    if let Err(e) = place_staged(data_dir, &journal) {
        return start_rollback(data_dir, journal, failed(&e));
    }
    commit(data_dir, journal)
}

fn snapshot_current(data_dir: &Path, journal: &RestoreJournal) -> Result<(), TransferError> {
    std::fs::create_dir_all(&journal.snapshot_dir)?;
    for name in backup_scope_entries() {
        let live = data_dir.join(&name);
        let kept = journal.snapshot_dir.join(&name);
        match (exists(&live)?, exists(&kept)?) {
            (true, false) => std::fs::rename(&live, &kept)?,
            (true, true) => return Err(blocked(&kept)),
            (false, _) => {}
        }
    }
    if journal.original_key {
        copy_atomically(&journal.credentials_key, &journal.snapshot_key_file())?;
    }
    sync_dir(data_dir)?;
    sync_dir(&journal.snapshot_dir)?;
    Ok(())
}

fn place_staged(data_dir: &Path, journal: &RestoreJournal) -> Result<(), TransferError> {
    for name in backup_scope_entries() {
        let is_staged = journal.staged_entries.contains(&name);
        let live = data_dir.join(&name);
        if !is_staged && exists(&live)? {
            return Err(blocked(&live));
        }
    }
    let payload = journal.payload_dir();
    for name in &journal.staged_entries {
        let staged = payload.join(name);
        let live = data_dir.join(name);
        match (exists(&staged)?, exists(&live)?) {
            (true, false) => std::fs::rename(&staged, &live)?,
            (true, true) => return Err(blocked(&live)),
            (false, true) => {}
            (false, false) => return Err(blocked(&staged)),
        }
    }
    if journal.staged_key {
        copy_atomically(&journal.staged_key_file(), &journal.credentials_key)?;
    } else {
        remove_file_if_present(&journal.credentials_key)?;
        sync_parent(&journal.credentials_key)?;
    }
    sync_dir(data_dir)?;
    sync_dir(&payload)?;
    Ok(())
}

fn commit(data_dir: &Path, mut journal: RestoreJournal) -> Result<RestoreOutcome, TransferError> {
    if journal.phase != RestorePhase::Committed {
        journal.advance(data_dir, RestorePhase::Committed)?;
    }
    RestoreMarker::clear(data_dir)?;
    RestoreJournal::clear(data_dir)?;
    let _ = remove_dir_all_if_present(&journal.staging_dir);
    Ok(RestoreOutcome::Applied {
        pre_restore_snapshot: journal.snapshot_dir,
    })
}

fn start_rollback(
    data_dir: &Path,
    mut journal: RestoreJournal,
    reason: RestoreRollback,
) -> Result<RestoreOutcome, TransferError> {
    journal.advance(
        data_dir,
        RestorePhase::RollingBack {
            reason: reason.clone(),
        },
    )?;
    roll_back(data_dir, &journal, reason)
}

fn roll_back(
    data_dir: &Path,
    journal: &RestoreJournal,
    reason: RestoreRollback,
) -> Result<RestoreOutcome, TransferError> {
    if exists(&journal.snapshot_dir)? {
        return_placed_entries(data_dir, journal)?;
    }
    for name in backup_scope_entries() {
        let kept = journal.snapshot_dir.join(&name);
        let live = data_dir.join(&name);
        match (exists(&kept)?, exists(&live)?) {
            (true, false) => std::fs::rename(&kept, &live)?,
            (true, true) => return Err(blocked(&live)),
            (false, _) => {}
        }
    }
    let kept_key = journal.snapshot_key_file();
    if !journal.original_key {
        remove_file_if_present(&journal.credentials_key)?;
        sync_parent(&journal.credentials_key)?;
    } else if exists(&kept_key)? {
        copy_atomically(&kept_key, &journal.credentials_key)?;
    }
    sync_dir(data_dir)?;
    remove_file_if_present(&kept_key)?;
    let _ = std::fs::remove_dir(&journal.snapshot_dir);
    sync_parent(&journal.snapshot_dir)?;
    RestoreMarker::clear(data_dir)?;
    RestoreJournal::clear(data_dir)?;
    let _ = remove_dir_all_if_present(&journal.staging_dir);
    Ok(RestoreOutcome::RolledBack { reason })
}

fn return_placed_entries(data_dir: &Path, journal: &RestoreJournal) -> Result<(), TransferError> {
    let payload = journal.payload_dir();
    for name in &journal.staged_entries {
        let staged = payload.join(name);
        let live = data_dir.join(name);
        if exists(&live)? && !exists(&staged)? {
            std::fs::rename(&live, &staged)?;
        }
    }
    Ok(())
}

fn abandon(data_dir: &Path, reason: RestoreRollback) -> Result<RestoreOutcome, TransferError> {
    RestoreMarker::clear(data_dir)?;
    let _ = remove_dir_all_if_present(&restore_staging_root(data_dir));
    Ok(RestoreOutcome::RolledBack { reason })
}

fn blocked(path: &Path) -> TransferError {
    TransferError::RestoreBlocked {
        path: path.display().to_string(),
    }
}

fn failed(error: &TransferError) -> RestoreRollback {
    RestoreRollback::Failed {
        error: error.to_string(),
    }
}
