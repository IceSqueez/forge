use std::fmt;
use std::path::PathBuf;

use forge_events::Event;
use forge_types::{ActionId, EventId};
use tokio::time::Instant;

use super::log_record::LogRecord;
use crate::launch::ForgeExit;
use crate::twitch::{RecordedRequest, RecordedSession, RecordedSubscription};

/// Evidence lists keep at most this many entries; counts stay exact.
pub const EVIDENCE_LIMIT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunClock {
    origin: Instant,
}

impl RunClock {
    pub fn starting_now() -> Self {
        Self {
            origin: Instant::now(),
        }
    }

    /// Milliseconds since the run started; instants before the start read as 0.
    pub fn millis(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.origin).as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioVerdict {
    Passed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct ScenarioOutcome {
    pub name: String,
    pub verdict: ScenarioVerdict,
    pub steps: Vec<StepOutcome>,
    /// `None` when the run was interrupted before forge was ready.
    pub forge: Option<ForgeEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Passed,
    Failed,
    Interrupted,
    NotRun,
}

/// Times are milliseconds on the run clock.
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub index: usize,
    pub keyword: &'static str,
    pub status: StepStatus,
    pub started_ms: Option<u64>,
    /// When the stimulus was delivered or the wait satisfied; every deadline counts from here.
    pub acted_ms: Option<u64>,
    pub action: Option<ActionReport>,
    pub expectations: Vec<ExpectationOutcome>,
}

#[derive(Debug, Clone)]
pub enum ActionReport {
    Done(ActionDetail),
    Failed {
        reason: String,
        ledger: Option<LedgerExcerpt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionDetail {
    ForgeReady {
        attempts: u32,
        server_port: u16,
    },
    TwitchSubscribed {
        session_ids: Vec<String>,
    },
    ChatSent {
        message_id: String,
    },
    CrowdSent {
        messages: usize,
    },
    SessionReconnected {
        from_session: String,
        to_session: String,
    },
    Paused,
    ActionRun {
        action_id: ActionId,
        /// forge publishes `action.start` with this id as its cause.
        execution_id: String,
    },
    GlobalSet,
}

#[derive(Debug, Clone)]
pub struct ExpectationOutcome {
    pub index: usize,
    pub keyword: &'static str,
    pub verdict: Verdict,
    pub deadline_ms: Option<u64>,
    pub evaluated_ms: Option<u64>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Passed,
    Failed(FailureCause),
    /// The step's action failed or the run was interrupted first.
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureCause {
    NotObserved {
        needed: u32,
        observed: usize,
    },
    WrongCount {
        expected: u32,
        observed: usize,
    },
    Present {
        observed: usize,
    },
    /// The server dropped or garbled pushes inside the window, so the stream cannot prove the claim.
    StreamGap {
        dropped: u64,
        undecodable: usize,
    },
    StreamClosed,
    UnresolvedName {
        name: String,
    },
    WrongCause {
        expected: EventId,
        actual: Option<EventId>,
    },
    NoSubscription,
    UnexpectedRequests {
        count: usize,
    },
    RequestCountOutOfRange {
        observed: usize,
        min: Option<u32>,
        max: Option<u32>,
    },
    NoFakeTwitch,
    NoLogLine,
    LogUnreadable {
        reason: String,
    },
}

impl fmt::Display for FailureCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotObserved { needed, observed } => {
                write!(
                    f,
                    "needed {needed} matching event(s) in time, observed {observed}"
                )
            }
            Self::WrongCount { expected, observed } => {
                write!(
                    f,
                    "expected exactly {expected} matching event(s), observed {observed}"
                )
            }
            Self::Present { observed } => {
                write!(
                    f,
                    "observed {observed} event(s) that were expected to be absent"
                )
            }
            Self::StreamGap {
                dropped,
                undecodable,
            } => write!(
                f,
                "the event stream lost {dropped} event(s) and garbled {undecodable} frame(s) in the window, so the claim cannot be decided"
            ),
            Self::StreamClosed => {
                write!(f, "the control connection closed before the window ended")
            }
            Self::UnresolvedName { name } => {
                write!(
                    f,
                    "`{name}` matched no event, so causation cannot be checked"
                )
            }
            Self::WrongCause { expected, actual } => match actual {
                Some(actual) => write!(f, "caused by {actual}, not {expected}"),
                None => write!(f, "carries no cause, expected {expected}"),
            },
            Self::NoSubscription => write!(f, "no live session held the subscription in time"),
            Self::UnexpectedRequests { count } => {
                write!(f, "the fake Twitch answered {count} unmodeled request(s)")
            }
            Self::RequestCountOutOfRange { observed, min, max } => {
                write!(f, "observed {observed} request(s), allowed")?;
                if let Some(min) = min {
                    write!(f, " at least {min}")?;
                }
                if let Some(max) = max {
                    write!(f, " at most {max}")?;
                }
                Ok(())
            }
            Self::NoFakeTwitch => write!(f, "the run has no fake Twitch"),
            Self::NoLogLine => write!(f, "no log line with those fields appeared in time"),
            Self::LogUnreadable { reason } => write!(f, "forge's log could not be read: {reason}"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Evidence {
    None,
    Events(EventEvidence),
    Causation(CausationEvidence),
    Ledger(LedgerExcerpt),
    Log(LogEvidence),
}

#[derive(Debug, Clone)]
pub struct JournaledEvent {
    pub arrived_ms: u64,
    pub event: Event,
}

#[derive(Debug, Clone)]
pub struct NearMiss {
    pub event: JournaledEvent,
    /// `source` or the JSON pointers whose matcher failed.
    pub mismatched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GapKind {
    Dropped(u64),
    Undecodable { frame: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub arrived_ms: u64,
    pub kind: GapKind,
}

#[derive(Debug, Clone, Default)]
pub struct EventEvidence {
    /// Matches inside the window; `samples` keeps the first few.
    pub matched: usize,
    pub samples: Vec<JournaledEvent>,
    /// Matches that arrived after the deadline.
    pub late: Vec<JournaledEvent>,
    pub near_misses: Vec<NearMiss>,
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Clone, Default)]
pub struct CausationEvidence {
    pub effect: Option<JournaledEvent>,
    pub cause: Option<JournaledEvent>,
    /// The effect's observed ancestors, nearest first.
    pub chain: Vec<JournaledEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct LedgerExcerpt {
    pub requests: Vec<RecordedRequest>,
    pub subscriptions: Vec<RecordedSubscription>,
    pub sessions: Vec<RecordedSession>,
}

#[derive(Debug, Clone, Default)]
pub struct LogEvidence {
    pub matched: Option<LogRecord>,
    /// Lines on the expected target whose fields differ.
    pub near_misses: Vec<LogRecord>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ForgeEvidence {
    pub run_root: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub pid: u32,
    pub attempts: u32,
    pub exited_during_run: bool,
    pub exit: Option<ForgeExit>,
    pub teardown_error: Option<String>,
    pub stderr_tail: Vec<String>,
    pub stdout_tail: Vec<String>,
    pub log_tail: Vec<String>,
}
