use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_storage::DataProvider;
use forge_transfer::{
    PRE_UPDATE_SNAPSHOT_EXTENSION, TransferExclusivity, TransferGuard, backup_file_stem,
    backups_dir, unique_suffix,
};
use forge_update::{ApplyMode, HandoffPlan, HandoffRequest, StagedArtifact, UpdatableInstall};
use time::OffsetDateTime;

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
    Failed(String),
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
) -> Result<(), String> {
    let _guard = guard;
    tokio::time::timeout(HANDOFF_PREPARE_BUDGET, write_handoff(backend, input))
        .await
        .unwrap_or_else(|_| Err("preparing the update took too long".to_owned()))
}

async fn write_handoff(backend: Arc<dyn DataProvider>, input: HandoffInput) -> Result<(), String> {
    let dir = backups_dir(&input.data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let snapshot =
        update_snapshot_path(&input.data_dir, OffsetDateTime::now_utc(), &unique_suffix());
    let written = snapshot_and_plan(backend, input, snapshot.clone()).await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(&snapshot).await;
    }
    written
}

async fn snapshot_and_plan(
    backend: Arc<dyn DataProvider>,
    input: HandoffInput,
    snapshot: PathBuf,
) -> Result<(), String> {
    backend
        .export(&snapshot)
        .await
        .map_err(|e| format!("database snapshot failed: {e}"))?;
    let schema_version = backend
        .schema_version()
        .await
        .map_err(|e| format!("schema version unavailable: {e}"))?;
    let running =
        std::env::current_exe().map_err(|e| format!("running executable unknown: {e}"))?;
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
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::*;

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
