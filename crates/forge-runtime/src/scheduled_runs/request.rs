use std::collections::BTreeMap;
use std::time::Duration;

use forge_storage::{MissedRunPolicy, ScheduledRunId, ScheduledRunSpec, StorageError};
use forge_types::{ActionId, EventId, Variant};
use time::OffsetDateTime;

use super::limits::{
    MAX_LATE_TOLERANCE, MAX_PENDING_SCHEDULED_RUNS, MAX_PENDING_SCHEDULED_RUNS_PER_ACTION,
    MAX_SCHEDULE_DELAY, MAX_SCHEDULE_KEY_CHARS, MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE,
    MIN_SCHEDULE_DELAY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleDue {
    After(Duration),
    At(OffsetDateTime),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleRequest {
    pub target_action_id: ActionId,
    pub due: ScheduleDue,
    pub key: Option<String>,
    pub missed_run_policy: MissedRunPolicy,
    pub args: BTreeMap<String, Variant>,
    pub scheduled_by_action: Option<ActionId>,
    pub scheduled_by_run: Option<String>,
    pub trigger_event_id: Option<EventId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledPlacement {
    pub id: ScheduledRunId,
    pub due_at: OffsetDateTime,
    pub superseded: Option<ScheduledRunId>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("the target action does not exist")]
    UnknownAction,
    #[error("the delay must be at least {} s", MIN_SCHEDULE_DELAY.as_secs())]
    DelayTooShort,
    #[error("the delay must be at most {} days", MAX_SCHEDULE_DELAY.as_secs() / SECONDS_PER_DAY)]
    DelayTooLong,
    #[error("the late tolerance must be between {} s and {} days", MIN_LATE_TOLERANCE.as_secs(), MAX_LATE_TOLERANCE.as_secs() / SECONDS_PER_DAY)]
    LateToleranceOutOfRange,
    #[error("the key is longer than {MAX_SCHEDULE_KEY_CHARS} characters")]
    KeyTooLong,
    #[error("the captured variables take {bytes} bytes, more than {MAX_SCHEDULED_ARGS_BYTES}")]
    ArgsTooLarge { bytes: usize },
    #[error("the captured variables cannot be stored: {0}")]
    ArgsNotStorable(#[from] serde_json::Error),
    #[error("{MAX_PENDING_SCHEDULED_RUNS} scheduled runs are already pending")]
    PendingCapReached,
    #[error(
        "{MAX_PENDING_SCHEDULED_RUNS_PER_ACTION} scheduled runs of this action are already pending"
    )]
    ActionPendingCapReached,
    #[error("the scheduler has stopped")]
    SchedulerStopped,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_DAY: u64 = 24 * 60 * SECONDS_PER_MINUTE;

pub fn skip_if_late_by_minutes(minutes: i64) -> MissedRunPolicy {
    let minutes = u64::try_from(minutes).unwrap_or(0);
    MissedRunPolicy::SkipIfLateBy(Duration::from_secs(
        minutes.saturating_mul(SECONDS_PER_MINUTE),
    ))
}

impl ScheduleRequest {
    pub(super) fn into_spec(
        self,
        now: OffsetDateTime,
        label: String,
    ) -> Result<ScheduledRunSpec, ScheduleError> {
        let due_at = match self.due {
            ScheduleDue::After(delay) => {
                check_delay(delay)?;
                now + delay
            }
            ScheduleDue::At(instant) => {
                check_delay(Duration::try_from(instant - now).unwrap_or(Duration::ZERO))?;
                instant
            }
        };
        check_policy(self.missed_run_policy)?;
        let key = normalized_key(self.key)?;
        check_args_size(&self.args)?;
        Ok(ScheduledRunSpec {
            target_action_id: self.target_action_id,
            due_at,
            key,
            missed_run_policy: self.missed_run_policy,
            args: self.args,
            scheduled_by_action: self.scheduled_by_action,
            scheduled_by_run: self.scheduled_by_run,
            trigger_event_id: self.trigger_event_id,
            scheduled_at: now,
            label,
        })
    }
}

fn check_delay(delay: Duration) -> Result<(), ScheduleError> {
    if delay < MIN_SCHEDULE_DELAY {
        return Err(ScheduleError::DelayTooShort);
    }
    if delay > MAX_SCHEDULE_DELAY {
        return Err(ScheduleError::DelayTooLong);
    }
    Ok(())
}

fn check_policy(policy: MissedRunPolicy) -> Result<(), ScheduleError> {
    match policy {
        MissedRunPolicy::RunLateOnce => Ok(()),
        MissedRunPolicy::SkipIfLateBy(tolerance)
            if (MIN_LATE_TOLERANCE..=MAX_LATE_TOLERANCE).contains(&tolerance) =>
        {
            Ok(())
        }
        MissedRunPolicy::SkipIfLateBy(_) => Err(ScheduleError::LateToleranceOutOfRange),
    }
}

pub(super) fn normalized_key(key: Option<String>) -> Result<Option<String>, ScheduleError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_SCHEDULE_KEY_CHARS {
        return Err(ScheduleError::KeyTooLong);
    }
    Ok(Some(trimmed.to_owned()))
}

fn check_args_size(args: &BTreeMap<String, Variant>) -> Result<(), ScheduleError> {
    let bytes = serde_json::to_vec(args)?.len();
    if bytes > MAX_SCHEDULED_ARGS_BYTES {
        return Err(ScheduleError::ArgsTooLarge { bytes });
    }
    Ok(())
}
