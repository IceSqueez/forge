use crate::error::RuntimeError;
use std::collections::BTreeMap;
use std::time::Duration;

use forge_registry::SECONDS_PER_MINUTE;
use forge_storage::{MissedRunPolicy, ScheduledRunId, ScheduledRunSpec};
use forge_types::{ActionId, EventId, Variant};
use time::OffsetDateTime;

use super::limits::{
    MAX_LATE_TOLERANCE, MAX_SCHEDULE_DELAY, MAX_SCHEDULE_KEY_CHARS, MAX_SCHEDULED_ARGS_BYTES,
    MIN_LATE_TOLERANCE, MIN_SCHEDULE_DELAY,
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
    pub scheduled_by_run: Option<EventId>,
    pub trigger_event_id: Option<EventId>,
    pub cause: Option<EventId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledPlacement {
    pub id: ScheduledRunId,
    pub due_at: OffsetDateTime,
    pub superseded: Option<ScheduledRunId>,
}

pub(crate) fn skip_if_late_by_minutes(minutes: i64) -> MissedRunPolicy {
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
        replay: bool,
    ) -> Result<ScheduledRunSpec, RuntimeError> {
        let due_at = match self.due {
            ScheduleDue::After(delay) => {
                check_delay(delay)?;
                now + delay
            }
            ScheduleDue::At(instant) => {
                match check_delay(Duration::try_from(instant - now).unwrap_or(Duration::ZERO)) {
                    Err(RuntimeError::DelayTooShort) => return Err(RuntimeError::DueTooSoon),
                    checked => checked?,
                }
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
            replay,
            scheduled_at: now,
            label,
        })
    }
}

fn check_delay(delay: Duration) -> Result<(), RuntimeError> {
    if delay < MIN_SCHEDULE_DELAY {
        return Err(RuntimeError::DelayTooShort);
    }
    if delay > MAX_SCHEDULE_DELAY {
        return Err(RuntimeError::DelayTooLong);
    }
    Ok(())
}

fn check_policy(policy: MissedRunPolicy) -> Result<(), RuntimeError> {
    match policy {
        MissedRunPolicy::RunLateOnce => Ok(()),
        MissedRunPolicy::SkipIfLateBy(tolerance)
            if (MIN_LATE_TOLERANCE..=MAX_LATE_TOLERANCE).contains(&tolerance) =>
        {
            Ok(())
        }
        MissedRunPolicy::SkipIfLateBy(_) => Err(RuntimeError::LateToleranceOutOfRange),
    }
}

pub(super) fn normalized_key(key: Option<String>) -> Result<Option<String>, RuntimeError> {
    let Some(key) = key else {
        return Ok(None);
    };
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_SCHEDULE_KEY_CHARS {
        return Err(RuntimeError::KeyTooLong);
    }
    Ok(Some(trimmed.to_owned()))
}

fn check_args_size(args: &BTreeMap<String, Variant>) -> Result<(), RuntimeError> {
    let bytes = serde_json::to_vec(args)?.len();
    if bytes > MAX_SCHEDULED_ARGS_BYTES {
        return Err(RuntimeError::ArgsTooLarge { bytes });
    }
    Ok(())
}
