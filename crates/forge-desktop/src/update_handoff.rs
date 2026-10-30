use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_storage::{DataProvider, StorageError};
use forge_transfer::{
    PRE_UPDATE_SNAPSHOT_EXTENSION, TransferExclusivity, TransferGuard, backup_file_stem,
    backups_dir, unique_suffix,
};
use forge_update::{ApplyMode, HandoffPlan, HandoffRequest, StagedArtifact, UpdatableInstall};
use time::OffsetDateTime;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::repo_failure::RepoFailure;
use crate::update_check::CURRENT_VERSION;

pub const HANDOFF_PREPARE_BUDGET: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct HandoffInput {
    pub install: UpdatableInstall,
    pub staged: StagedArtifact,
    pub to_version: String,
    pub mode: ApplyMode,
    pub data_dir: PathBuf,
    pub exclusivity: TransferExclusivity,
}

pub fn update_snapshot_path(data_dir: &Path, now: OffsetDateTime, suffix: &str) -> PathBuf {
    backups_dir(data_dir).join(format!(
        "{}.{PRE_UPDATE_SNAPSHOT_EXTENSION}",
        backup_file_stem(now, suffix)
    ))
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandoffError {
    Busy,
    Failed(RepoFailure),
}

pub async fn prepare_handoff(
    backend: Arc<dyn DataProvider>,
    input: HandoffInput,
) -> Result<(), HandoffError> {
    let Some(guard) = input.exclusivity.try_acquire() else {
        return Err(HandoffError::Busy);
    };
    prepare_handoff_holding(backend, input, guard)
        .await
        .map_err(HandoffError::Failed)
}

pub async fn prepare_handoff_holding(
    backend: Arc<dyn DataProvider>,
    input: HandoffInput,
    guard: TransferGuard,
) -> Result<(), RepoFailure> {
    let _guard = guard;
    write_handoff(backend, input).await
}

async fn write_handoff(
    backend: Arc<dyn DataProvider>,
    input: HandoffInput,
) -> Result<(), RepoFailure> {
    let dir = backups_dir(&input.data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(RepoFailure::from)?;
    let snapshot =
        update_snapshot_path(&input.data_dir, OffsetDateTime::now_utc(), &unique_suffix());
    let deadline = Instant::now() + HANDOFF_PREPARE_BUDGET;
    let exporter = Arc::clone(&backend);
    let target = snapshot.clone();
    let mut export = tokio::spawn(async move { exporter.export(&target).await });
    let exported = match tokio::time::timeout_at(deadline, &mut export).await {
        Ok(joined) => joined
            .map_err(RepoFailure::from)
            .and_then(|exported| exported.map_err(RepoFailure::from)),
        Err(elapsed) => {
            tokio::spawn(discard_once_settled(export, snapshot.clone()));
            Err(RepoFailure::operation(elapsed))
        }
    };
    let written = match exported {
        Ok(()) => write_plan(backend, input, snapshot.clone(), deadline).await,
        Err(failure) => Err(failure),
    };
    if written.is_err() {
        discard_snapshot(&snapshot).await;
    }
    written
}

async fn discard_snapshot(snapshot: &Path) {
    if let Err(error) = tokio::fs::remove_file(snapshot).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::debug!(%error, path = %snapshot.display(), "partial update snapshot could not be removed");
    }
}

async fn discard_once_settled(export: JoinHandle<Result<(), StorageError>>, snapshot: PathBuf) {
    match export.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::debug!(%error, "abandoned update snapshot failed"),
        Err(error) => tracing::debug!(%error, "abandoned update snapshot did not finish"),
    }
    discard_snapshot(&snapshot).await;
}

async fn write_plan(
    backend: Arc<dyn DataProvider>,
    input: HandoffInput,
    snapshot: PathBuf,
    deadline: Instant,
) -> Result<(), RepoFailure> {
    let schema_version = tokio::time::timeout_at(deadline, backend.schema_version())
        .await
        .map_err(RepoFailure::operation)?
        .map_err(RepoFailure::from)?;
    let running = std::env::current_exe().map_err(RepoFailure::from)?;
    tokio::task::spawn_blocking(move || {
        let plan = HandoffPlan::new(HandoffRequest {
            install: &input.install,
            staged: &input.staged,
            from_version: CURRENT_VERSION,
            to_version: &input.to_version,
            database_snapshot: snapshot,
            schema_version,
            running_executable: &running,
            mode: input.mode,
        })?;
        plan.write(&input.data_dir)
    })
    .await
    .map_err(RepoFailure::from)?
    .map_err(RepoFailure::from)
}

#[cfg(test)]
mod tests {
    use forge_update::InstallLayout;
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::*;
    use crate::test_support::{
        pump, runtime, test_backend, test_backend_with_gated_export, test_backend_with_late_export,
        test_backend_with_stalling_export,
    };

    const STAGED_SIZE: u64 = 100;

    fn input(data_dir: &Path, exclusivity: &TransferExclusivity) -> HandoffInput {
        HandoffInput {
            install: UpdatableInstall {
                layout: InstallLayout::LinuxAppImage,
                target: data_dir.join("forge.AppImage"),
            },
            staged: StagedArtifact {
                path: data_dir.join(".forge-staged-forge.AppImage"),
                size: STAGED_SIZE,
            },
            to_version: "0.9.0".to_owned(),
            mode: ApplyMode::Restart,
            data_dir: data_dir.to_path_buf(),
            exclusivity: exclusivity.clone(),
        }
    }

    fn backups_entries(data_dir: &Path) -> usize {
        std::fs::read_dir(backups_dir(data_dir)).map_or(0, Iterator::count)
    }

    #[test]
    fn a_handoff_while_a_backup_or_restore_runs_fails_without_writing_anything() {
        let rt = runtime();
        let data = tempfile::tempdir().unwrap();
        let exclusivity = TransferExclusivity::new();
        let _running_backup = exclusivity.try_acquire().unwrap();
        let (backend, _) = test_backend();

        let result = rt.block_on(prepare_handoff(backend, input(data.path(), &exclusivity)));

        assert_eq!(
            (result, backups_entries(data.path())),
            (Err(HandoffError::Busy), 0)
        );
    }

    #[test]
    fn a_handoff_holds_off_backups_and_restores_until_it_settles() {
        let rt = runtime();
        let data = tempfile::tempdir().unwrap();
        let exclusivity = TransferExclusivity::new();
        let (backend, gate) = test_backend_with_gated_export();
        let handoff = rt.spawn(prepare_handoff(backend, input(data.path(), &exclusivity)));
        pump(&rt);
        let held_while_snapshotting = exclusivity.is_held();

        gate.notify_one();
        let result = rt.block_on(handoff).unwrap();

        assert_eq!(
            (
                held_while_snapshotting,
                result.is_err(),
                exclusivity.is_held()
            ),
            (true, true, false)
        );
    }

    #[test]
    fn a_snapshot_that_runs_out_of_budget_fails_and_leaves_no_partial_file() {
        let rt = runtime();
        let data = tempfile::tempdir().unwrap();
        let exclusivity = TransferExclusivity::new();
        let backend = test_backend_with_stalling_export();

        let result = rt.block_on(async {
            tokio::time::pause();
            prepare_handoff(backend, input(data.path(), &exclusivity)).await
        });

        assert_eq!(
            (
                matches!(result, Err(HandoffError::Failed(_))),
                backups_entries(data.path()),
                exclusivity.is_held(),
            ),
            (true, 0, false)
        );
    }

    #[test]
    fn a_snapshot_that_finishes_after_the_budget_is_removed_when_it_lands() {
        let rt = runtime();
        let data = tempfile::tempdir().unwrap();
        let exclusivity = TransferExclusivity::new();
        let (backend, gate) = test_backend_with_late_export();

        let result = rt.block_on(async {
            tokio::time::pause();
            let result = prepare_handoff(backend, input(data.path(), &exclusivity)).await;
            gate.notify_one();
            tokio::time::sleep(HANDOFF_PREPARE_BUDGET).await;
            result
        });

        assert_eq!(
            (
                matches!(result, Err(HandoffError::Failed(_))),
                backups_entries(data.path()),
            ),
            (true, 0)
        );
    }

    #[test]
    fn the_update_snapshot_lands_in_the_backups_dir_as_a_stamped_pre_update_database() {
        let data_dir = Path::new("/data/forge");
        let now = PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::October, 15).unwrap(),
            Time::from_hms(9, 5, 7).unwrap(),
        )
        .assume_utc();

        let path = update_snapshot_path(data_dir, now, "a1b2c3");

        assert_eq!(
            path,
            backups_dir(data_dir).join("forge-backup-20261015-090507Z-a1b2c3.pre-update.db")
        );
    }
}
