use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use crate::UpdateError;
use crate::apply::files::{FileRetry, remove_path, restore_previous_build};
use crate::apply::{ApplyContext, apply_update};
use crate::apply_state::{ApplyMode, ApplyPhase, ApplyState};
use crate::database::{DatabaseUnit, SchemaProbe, restore_database};
use crate::health::{HealthMarker, clear_health_marker, read_health_marker};
use crate::journal::UpdateJournal;
use crate::process::{ChildProcess, Clock, ProcessHost};
use crate::report::{ApplyFailure, DatabaseRestore, RollbackReason, UpdateReport, record_outcome};
use crate::supervisor_lock::SupervisorLock;

mod launch;
mod startup;

pub use launch::{CANDIDATE_FLAG, LaunchRole, SUPERVISOR_FLAG, launch_role, launch_supervisor};
pub use startup::{StartAction, pending_start_action};

const DEFAULT_HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_PARENT_EXIT_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_INSTALLER_TIMEOUT: Duration = Duration::from_secs(600);
const DEFAULT_CHILD_EXIT_GRACE: Duration = Duration::from_secs(10);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DEFAULT_FILE_RETRY_ATTEMPTS: u32 = 20;
const DEFAULT_FILE_RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisorSettings {
    pub health_timeout: Duration,
    pub parent_exit_timeout: Duration,
    pub installer_timeout: Duration,
    pub child_exit_grace: Duration,
    pub poll_interval: Duration,
    pub file_retry_attempts: u32,
    pub file_retry_delay: Duration,
}

impl Default for SupervisorSettings {
    fn default() -> Self {
        Self {
            health_timeout: DEFAULT_HEALTH_TIMEOUT,
            parent_exit_timeout: DEFAULT_PARENT_EXIT_TIMEOUT,
            installer_timeout: DEFAULT_INSTALLER_TIMEOUT,
            child_exit_grace: DEFAULT_CHILD_EXIT_GRACE,
            poll_interval: DEFAULT_POLL_INTERVAL,
            file_retry_attempts: DEFAULT_FILE_RETRY_ATTEMPTS,
            file_retry_delay: DEFAULT_FILE_RETRY_DELAY,
        }
    }
}

pub struct SupervisorContext<'a> {
    pub data_dir: &'a Path,
    pub database: &'a DatabaseUnit,
    pub settings: SupervisorSettings,
    pub host: &'a mut dyn ProcessHost,
    pub clock: &'a dyn Clock,
    pub schema: &'a dyn SchemaProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorOutcome {
    NothingPending,
    Updated,
    AwaitingFirstStart,
    RolledBack,
    RollbackFailed,
    NotApplied,
}

enum Verdict {
    Healthy,
    Unhealthy(RollbackReason),
}

pub fn run_supervisor(context: SupervisorContext<'_>) -> Result<SupervisorOutcome, UpdateError> {
    let Some(_lock) = SupervisorLock::try_acquire(context.data_dir)? else {
        return Err(UpdateError::SupervisorBusy);
    };
    let data_dir = context.data_dir;
    let (Some(journal), Some(state)) = (
        UpdateJournal::read_blocking(data_dir)?,
        ApplyState::read(data_dir)?,
    ) else {
        UpdateJournal::clear_blocking(data_dir)?;
        ApplyState::clear(data_dir)?;
        return Ok(SupervisorOutcome::NothingPending);
    };
    clear_health_marker(data_dir)?;
    let mut supervisor = Supervisor {
        context,
        journal,
        state,
    };
    match supervisor.state.phase {
        ApplyPhase::HandedOff | ApplyPhase::Applying => supervisor.apply_then_supervise(),
        ApplyPhase::AwaitingFirstStart | ApplyPhase::AwaitingHealth => {
            supervisor.supervise_after_handoff()
        }
    }
}

struct Supervisor<'a> {
    context: SupervisorContext<'a>,
    journal: UpdateJournal,
    state: ApplyState,
}

impl Supervisor<'_> {
    fn retry(&self) -> FileRetry<'_> {
        FileRetry {
            clock: self.context.clock,
            attempts: self.context.settings.file_retry_attempts,
            delay: self.context.settings.file_retry_delay,
        }
    }

    fn set_phase(&mut self, phase: ApplyPhase) -> Result<(), UpdateError> {
        self.state.phase = phase;
        self.state.write(self.context.data_dir)
    }

    fn apply_then_supervise(&mut self) -> Result<SupervisorOutcome, UpdateError> {
        let settings = self.context.settings;
        if !self
            .context
            .host
            .wait_for_parent_exit(settings.parent_exit_timeout)?
        {
            return self.finish_not_applied(ApplyFailure::ParentStillRunning, false);
        }
        self.set_phase(ApplyPhase::Applying)?;
        let applied = apply_update(
            &self.journal,
            &self.state,
            ApplyContext {
                host: &mut *self.context.host,
                clock: self.context.clock,
                installer_timeout: settings.installer_timeout,
                poll_interval: settings.poll_interval,
                retry: FileRetry {
                    clock: self.context.clock,
                    attempts: settings.file_retry_attempts,
                    delay: settings.file_retry_delay,
                },
            },
        );
        if let Err(error) = applied {
            let relaunch = self.state.mode == ApplyMode::Restart;
            return self.finish_not_applied(ApplyFailure::from_error(&error), relaunch);
        }
        match self.state.mode {
            ApplyMode::OnQuit => {
                self.set_phase(ApplyPhase::AwaitingFirstStart)?;
                Ok(SupervisorOutcome::AwaitingFirstStart)
            }
            ApplyMode::Restart => self.supervise_candidate(),
        }
    }

    fn supervise_after_handoff(&mut self) -> Result<SupervisorOutcome, UpdateError> {
        let timeout = self.context.settings.parent_exit_timeout;
        if !self.context.host.wait_for_parent_exit(timeout)? {
            return Err(UpdateError::ParentStillRunning);
        }
        self.supervise_candidate()
    }

    fn supervise_candidate(&mut self) -> Result<SupervisorOutcome, UpdateError> {
        self.set_phase(ApplyPhase::AwaitingHealth)?;
        clear_health_marker(self.context.data_dir)?;
        let launched = self.context.host.spawn(
            &self.state.launch_executable,
            &[OsString::from(CANDIDATE_FLAG)],
        );
        let mut candidate = match launched {
            Ok(candidate) => candidate,
            Err(error) => {
                return self.roll_back(RollbackReason::LaunchFailed {
                    detail: error.to_string(),
                });
            }
        };
        match self.watch_health(candidate.as_mut())? {
            Verdict::Healthy => self.confirm(),
            Verdict::Unhealthy(reason) => {
                self.stop(candidate.as_mut());
                self.roll_back(reason)
            }
        }
    }

    fn watch_health(&self, candidate: &mut dyn ChildProcess) -> Result<Verdict, UpdateError> {
        let settings = self.context.settings;
        let clock = self.context.clock;
        let started = clock.now();
        loop {
            if self.marker_confirms() {
                return Ok(Verdict::Healthy);
            }
            if let Some(exit) = candidate.try_exit()? {
                if exit.is_success() || self.marker_confirms() {
                    return Ok(Verdict::Healthy);
                }
                return Ok(Verdict::Unhealthy(RollbackReason::Crashed {
                    exit_code: exit.code(),
                }));
            }
            if clock.now().saturating_duration_since(started) >= settings.health_timeout {
                return Ok(Verdict::Unhealthy(RollbackReason::NoHealthSignal));
            }
            clock.sleep(settings.poll_interval);
        }
    }

    fn marker_confirms(&self) -> bool {
        read_health_marker(self.context.data_dir)
            .ok()
            .flatten()
            .is_some_and(|marker: HealthMarker| marker.is_for_version(&self.journal.to_version))
    }

    fn stop(&self, candidate: &mut dyn ChildProcess) {
        let settings = self.context.settings;
        let clock = self.context.clock;
        if candidate.kill().is_err() {
            return;
        }
        let started = clock.now();
        while matches!(candidate.try_exit(), Ok(None)) {
            if clock.now().saturating_duration_since(started) >= settings.child_exit_grace {
                break;
            }
            clock.sleep(settings.poll_interval);
        }
    }

    fn confirm(&mut self) -> Result<SupervisorOutcome, UpdateError> {
        let previous = self.journal.previous_build.clone();
        let _ = self.retry().run(|| remove_path(&previous));
        self.clear_records()?;
        Ok(SupervisorOutcome::Updated)
    }

    fn roll_back(&mut self, reason: RollbackReason) -> Result<SupervisorOutcome, UpdateError> {
        let data_dir = self.context.data_dir;
        let restored = restore_previous_build(
            &self.journal.install_target,
            &self.journal.previous_build,
            self.retry(),
        );
        if let Err(error) = restored {
            record_outcome(
                data_dir,
                UpdateReport::RollbackFailed {
                    failed_version: self.journal.to_version.clone(),
                    reason,
                    detail: error.to_string(),
                },
            )?;
            self.clear_records()?;
            return Ok(SupervisorOutcome::RollbackFailed);
        }
        let database = self.restore_database_if_migrated();
        record_outcome(
            data_dir,
            UpdateReport::RolledBack {
                failed_version: self.journal.to_version.clone(),
                restored_version: self.journal.from_version.clone(),
                reason,
                database,
            },
        )?;
        self.clear_records()?;
        self.relaunch()?;
        Ok(SupervisorOutcome::RolledBack)
    }

    fn restore_database_if_migrated(&self) -> DatabaseRestore {
        let data_dir = self.context.data_dir;
        let schema_changed = self
            .context
            .schema
            .current_schema_version(data_dir)
            .is_none_or(|current| current != self.journal.schema_version);
        if !schema_changed {
            return DatabaseRestore::Unchanged;
        }
        match restore_database(
            data_dir,
            self.context.database,
            &self.journal.database_snapshot,
        ) {
            Ok(()) => DatabaseRestore::Restored,
            Err(error) => DatabaseRestore::Failed {
                detail: error.to_string(),
            },
        }
    }

    fn finish_not_applied(
        &mut self,
        reason: ApplyFailure,
        relaunch: bool,
    ) -> Result<SupervisorOutcome, UpdateError> {
        let staged = self.state.staged_artifact.clone();
        let _ = self.retry().run(|| remove_path(&staged));
        record_outcome(
            self.context.data_dir,
            UpdateReport::NotApplied {
                version: self.journal.to_version.clone(),
                reason,
            },
        )?;
        self.clear_records()?;
        if relaunch {
            self.relaunch()?;
        }
        Ok(SupervisorOutcome::NotApplied)
    }

    fn relaunch(&mut self) -> Result<(), UpdateError> {
        self.context
            .host
            .spawn(&self.state.launch_executable, &[])
            .map(drop)
    }

    fn clear_records(&self) -> Result<(), UpdateError> {
        let data_dir = self.context.data_dir;
        clear_health_marker(data_dir)?;
        ApplyState::clear(data_dir)?;
        UpdateJournal::clear_blocking(data_dir)
    }
}
