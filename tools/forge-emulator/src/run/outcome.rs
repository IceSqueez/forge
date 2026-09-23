use std::fmt;
use std::path::PathBuf;

use forge_events::Event;
use forge_types::{ActionId, EventId};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use super::log_record::LogRecord;
use crate::fixture::Redactions;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioVerdict {
    Passed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioOutcome {
    pub name: String,
    pub verdict: ScenarioVerdict,
    pub steps: Vec<StepOutcome>,
    /// `None` when the run was interrupted before forge was ready.
    pub forge: Option<ForgeEvidence>,
    #[serde(skip)]
    pub redactions: Redactions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Passed,
    Failed,
    Interrupted,
    NotRun,
}

/// Times are milliseconds on the run clock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepOutcome {
    pub index: usize,
    pub keyword: String,
    pub status: StepStatus,
    pub started_ms: Option<u64>,
    /// When the stimulus was delivered or the wait satisfied; every deadline counts from here.
    pub acted_ms: Option<u64>,
    pub action: Option<ActionReport>,
    pub expectations: Vec<ExpectationOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionReport {
    Done(ActionDetail),
    Failed {
        reason: String,
        ledger: Option<LedgerExcerpt>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    TwitchEventDelivered {
        subscription_type: String,
        sessions: usize,
    },
    SessionReconnected {
        from_session: String,
        to_session: String,
    },
    Paused,
    OverlayPageOpened {
        overlay: String,
        identity: String,
    },
    ActionRun {
        action_id: ActionId,
        /// forge publishes `action.start` with this id as its cause.
        execution_id: String,
    },
    GlobalSet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectationOutcome {
    pub index: usize,
    pub keyword: String,
    pub verdict: Verdict,
    pub deadline_ms: Option<u64>,
    pub evaluated_ms: Option<u64>,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Passed,
    Failed(FailureCause),
    /// The step's action failed or the run was interrupted first.
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// No step opened a page for the overlay the expectation names.
    NoOverlayPage {
        overlay: String,
    },
    NoOverlayContent {
        observed: usize,
    },
    /// Content reached the page as tagged `Variant` JSON, which a browser renders as
    /// `[object Object]`; the pointers name where.
    TaggedOverlayValue {
        pointers: Vec<String>,
    },
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
                    "observed at least {observed} event(s) that were expected to be absent"
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
            Self::NoOverlayPage { overlay } => {
                write!(f, "no page is open for overlay `{overlay}`")
            }
            Self::NoOverlayContent { observed } => write!(
                f,
                "no content frame carrying those values arrived in time, observed {observed}"
            ),
            Self::TaggedOverlayValue { pointers } => write!(
                f,
                "the content frame carries tagged Variant JSON at {}, which renders as [object Object]",
                pointers.join(", ")
            ),
            Self::NoLogLine => write!(f, "no log line with those fields appeared in time"),
            Self::LogUnreadable { reason } => write!(f, "forge's log could not be read: {reason}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    None,
    Events(EventEvidence),
    Causation(CausationEvidence),
    Ledger(LedgerExcerpt),
    Overlay(OverlayEvidence),
    Log(LogEvidence),
}

/// One content frame exactly as it came off the wire, so a report shows what the page had to
/// render rather than a re-serialization of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceivedContent {
    pub arrived_ms: u64,
    pub content: serde_json::Value,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OverlayEvidence {
    pub overlay: String,
    /// Content frames in the window; other frame shapes are not evidence for this claim.
    pub frames: Vec<ReceivedContent>,
    /// Keys of the closest frame whose value differs from the expected literal.
    pub mismatched: Vec<String>,
    /// Pointers into the closest frame's content that hold tagged Variant JSON.
    pub tagged: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournaledEvent {
    pub arrived_ms: u64,
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearMiss {
    pub event: JournaledEvent,
    /// `source` or the JSON pointers whose matcher failed.
    pub mismatched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapKind {
    Dropped(u64),
    Undecodable { frame: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    pub arrived_ms: u64,
    pub kind: GapKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventEvidence {
    /// Matches inside the window; `samples` keeps the first few.
    pub matched: usize,
    pub samples: Vec<JournaledEvent>,
    /// Matches that arrived after the deadline.
    pub late: Vec<JournaledEvent>,
    pub near_misses: Vec<NearMiss>,
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CausationEvidence {
    pub effect: Option<JournaledEvent>,
    pub cause: Option<JournaledEvent>,
    /// The effect's observed ancestors, nearest first.
    pub chain: Vec<JournaledEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerExcerpt {
    pub requests: Vec<RecordedRequest>,
    pub subscriptions: Vec<RecordedSubscription>,
    pub sessions: Vec<RecordedSession>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogEvidence {
    pub matched: Option<LogRecord>,
    /// Lines on the expected target whose fields differ.
    pub near_misses: Vec<LogRecord>,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeEvidence {
    pub run_root: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub pid: u32,
    /// As forge's server reports it; `None` when it would not say.
    pub version: Option<String>,
    pub attempts: u32,
    pub exited_during_run: bool,
    pub exit: Option<ForgeExit>,
    pub teardown_error: Option<String>,
    pub stderr_tail: Vec<String>,
    pub stdout_tail: Vec<String>,
    pub log_tail: Vec<String>,
}
