use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

use forge_platform_core::paths;
use forge_storage::{DATABASE_FILE_NAME, database_unit_file_names};
use forge_update::{
    DatabaseUnit, LaunchRole, SchemaProbe, StartAction, SupervisorContext, SupervisorOutcome,
    SupervisorSettings, SystemClock, SystemProcessHost, UpdateError, launch_role,
    launch_supervisor, pending_start_action, run_supervisor,
};

use crate::update_check::CURRENT_VERSION;

const EXIT_SUCCESS: i32 = 0;
const EXIT_SUPERVISOR_ERROR: i32 = 1;
const EXIT_UPDATE_NOT_KEPT: i32 = 3;
const EXIT_ROLLBACK_FAILED: i32 = 4;

const SUPERVISOR_LOG_DIR: &str = "logs";
const SUPERVISOR_LOG_FILE: &str = "update-supervisor.log";

pub enum StartDecision {
    Boot { deferred_error: Option<UpdateError> },
    Exit(i32),
}

pub fn decide_start(args: impl IntoIterator<Item = OsString>) -> StartDecision {
    match launch_role(args) {
        LaunchRole::Supervisor { data_dir } => StartDecision::Exit(supervise(&data_dir)),
        LaunchRole::Candidate => StartDecision::Boot {
            deferred_error: None,
        },
        LaunchRole::Regular => regular_start(&paths::data_dir()),
    }
}

fn regular_start(data_dir: &Path) -> StartDecision {
    match pending_start_action(data_dir, CURRENT_VERSION) {
        Ok(StartAction::Continue) => StartDecision::Boot {
            deferred_error: None,
        },
        Ok(StartAction::SupervisorRunning) => StartDecision::Exit(EXIT_SUCCESS),
        Ok(StartAction::HandOff) => match launch_supervisor(data_dir) {
            Ok(_) => StartDecision::Exit(EXIT_SUCCESS),
            Err(error) => StartDecision::Boot {
                deferred_error: Some(error),
            },
        },
        Err(error) => StartDecision::Boot {
            deferred_error: Some(error),
        },
    }
}

struct StoredSchemaVersion;

impl SchemaProbe for StoredSchemaVersion {
    fn current_schema_version(&self, data_dir: &Path) -> Option<u32> {
        forge_storage_sqlite::read_applied_schema_version(&data_dir.join(DATABASE_FILE_NAME))
            .ok()
            .flatten()
    }
}

fn supervise(data_dir: &Path) -> i32 {
    init_supervisor_logging(data_dir);
    let Some(database) = DatabaseUnit::from_file_names(database_unit_file_names()) else {
        tracing::error!("update supervisor has no database file names; nothing was changed");
        return EXIT_SUPERVISOR_ERROR;
    };
    let mut host = SystemProcessHost::new();
    let outcome = run_supervisor(SupervisorContext {
        data_dir,
        database: &database,
        settings: SupervisorSettings::default(),
        host: &mut host,
        clock: &SystemClock,
        schema: &StoredSchemaVersion,
    });
    match outcome {
        Ok(outcome) => {
            tracing::info!(?outcome, "update supervisor finished");
            supervisor_exit_code(outcome)
        }
        Err(error) => {
            tracing::error!(error = %error, "update supervisor failed");
            EXIT_SUPERVISOR_ERROR
        }
    }
}

pub fn supervisor_exit_code(outcome: SupervisorOutcome) -> i32 {
    match outcome {
        SupervisorOutcome::NothingPending
        | SupervisorOutcome::Updated
        | SupervisorOutcome::AwaitingFirstStart => EXIT_SUCCESS,
        SupervisorOutcome::RolledBack | SupervisorOutcome::NotApplied => EXIT_UPDATE_NOT_KEPT,
        SupervisorOutcome::RollbackFailed => EXIT_ROLLBACK_FAILED,
    }
}

fn init_supervisor_logging(data_dir: &Path) {
    let log_dir = data_dir.join(SUPERVISOR_LOG_DIR);
    let file = std::fs::create_dir_all(&log_dir).and_then(|()| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(SUPERVISOR_LOG_FILE))
    });
    let builder = tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false);
    match file {
        Ok(file) => builder.with_writer(Mutex::new(file)).init(),
        Err(_) => builder.with_writer(std::io::stderr).init(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use forge_storage::{DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION};
    use forge_update::{SchemaProbe, SupervisorOutcome};

    use super::{EXIT_SUPERVISOR_ERROR, StoredSchemaVersion, supervisor_exit_code};
    use crate::EXIT_USAGE_ERROR;

    const TEST_KEY: [u8; 32] = [0x5a; 32];

    #[test]
    fn supervisor_exits_zero_only_when_the_new_version_is_kept_or_nothing_was_due() {
        for outcome in [
            SupervisorOutcome::NothingPending,
            SupervisorOutcome::Updated,
            SupervisorOutcome::AwaitingFirstStart,
        ] {
            assert_eq!(supervisor_exit_code(outcome), 0, "for {outcome:?}");
        }
        for outcome in [
            SupervisorOutcome::RolledBack,
            SupervisorOutcome::NotApplied,
            SupervisorOutcome::RollbackFailed,
        ] {
            let code = supervisor_exit_code(outcome);
            assert!(
                code != 0 && code != EXIT_USAGE_ERROR && code != EXIT_SUPERVISOR_ERROR,
                "{outcome:?} exits {code}, indistinguishable from success, usage or supervisor error"
            );
        }
    }

    #[test]
    fn a_failed_rollback_exits_differently_from_a_completed_one() {
        assert_ne!(
            supervisor_exit_code(SupervisorOutcome::RollbackFailed),
            supervisor_exit_code(SupervisorOutcome::RolledBack),
        );
    }

    #[test]
    fn schema_probe_reports_no_version_for_a_missing_or_unreadable_database() {
        for contents in [
            None,
            Some(&b""[..]),
            Some(&b"this is not a sqlite database, only some bytes on disk"[..]),
            Some(&[0xff_u8; 4096][..]),
        ] {
            let dir = tempfile::tempdir().unwrap();
            if let Some(bytes) = contents {
                std::fs::write(dir.path().join(DATABASE_FILE_NAME), bytes).unwrap();
            }
            assert_eq!(
                StoredSchemaVersion.current_schema_version(dir.path()),
                None,
                "for {contents:?}"
            );
        }
    }

    #[test]
    fn schema_probe_reads_the_version_of_the_migrated_database_in_the_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let media = dir.path().join("media");
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let backend =
                    forge_storage_sqlite::SqliteBackend::open_for_test(&url, TEST_KEY, media, None)
                        .await
                        .unwrap();
                backend.shutdown().await;
            });

        assert_eq!(
            StoredSchemaVersion.current_schema_version(dir.path()),
            Some(EXPECTED_SCHEMA_VERSION)
        );
    }
}
