use forge_script::ScriptError;
use forge_storage::{OverlayId, RunRefusal, StorageError};
use forge_types::{EventId, QueueId};
use thiserror::Error;

use crate::engine::scheduled_runs::limits::{
    MAX_LATE_TOLERANCE, MAX_PENDING_SCHEDULED_RUNS, MAX_PENDING_SCHEDULED_RUNS_PER_ACTION,
    MAX_SCHEDULE_DELAY, MAX_SCHEDULE_KEY_CHARS, MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE,
    MIN_SCHEDULE_DELAY,
};
use crate::services::actions::OverlayWiringRefusal;
use forge_registry::SECONDS_PER_DAY;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Script(#[from] ScriptError),

    #[error(transparent)]
    Overlay(#[from] forge_overlay::OverlayError),

    #[error("condition evaluation did not run to completion")]
    ConditionCanceled,

    #[error("condition references undefined variables: {}", .names.join(", "))]
    UndefinedVariables { names: Vec<String> },

    #[error("event {0} not found in ring or persistent log")]
    EventNotFound(EventId),

    #[error("compile: {0}")]
    ScriptCompile(String),

    #[error("scheduler channel closed")]
    SchedulerChannelClosed,

    #[error("queue not found: {0}")]
    QueueNotFound(QueueId),

    #[error("queue refused the run: {0}")]
    QueueRefused(RunRefusal),

    #[error("engine channel closed")]
    EngineChannelClosed,

    #[error("engine stopped before the step reported an outcome")]
    EngineNoOutcome,

    #[error("invalid url")]
    InvalidUrl,

    #[error("only http and https urls are allowed")]
    SchemeNotAllowed,

    #[error("blocked: target resolves to a private or local address")]
    BlockedAddress,

    #[error("host did not resolve")]
    UnresolvableHost,

    #[error("too many redirects")]
    TooManyRedirects,

    #[error("request timed out")]
    EgressTimeout,

    #[error("network error: {0}")]
    EgressNetwork(String),

    #[error("the target action does not exist")]
    ScheduleUnknownAction,

    #[error("the delay must be at least {} s", MIN_SCHEDULE_DELAY.as_secs())]
    DelayTooShort,

    #[error("the date-time must be at least {} s from now", MIN_SCHEDULE_DELAY.as_secs())]
    DueTooSoon,

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

    #[error("overlay wiring refused: {0:?}")]
    OverlayWiringRefused(OverlayWiringRefusal),

    #[error("nothing was written: {source}")]
    OverlayWiringNothingWritten {
        #[source]
        source: StorageError,
    },

    #[error("no overlay is stored as '{0}'")]
    UnknownOverlay(OverlayId),

    #[error("overlay '{id}' needs an overlay type this build does not carry: {kind_id}")]
    UnavailableOverlayKind { id: OverlayId, kind_id: String },

    #[error("overlay file work did not finish")]
    OverlayFileWorkInterrupted,

    #[error("overlay '{id}' already has {capacity} shows waiting, so this one was not queued")]
    ShowQueueFull { id: OverlayId, capacity: usize },

    #[error("overlay '{0}' shows a latest value and takes no content from actions")]
    SlotBound(OverlayId),

    #[error("unknown latest slot '{0}'")]
    UnknownLatestSlot(String),

    #[error("os service unavailable: {0}")]
    OsServiceUnavailable(String),

    #[error("os operation failed: {0}")]
    OsOperationFailed(String),

    #[error("unix timestamp out of range: {0}")]
    TimestampOutOfRange(#[from] time::error::ComponentRange),

    #[error("cannot parse '{0}' as a datetime or unix timestamp")]
    UnreadableDatetime(String),
}

impl From<OverlayWiringRefusal> for RuntimeError {
    fn from(refusal: OverlayWiringRefusal) -> Self {
        Self::OverlayWiringRefused(refusal)
    }
}
