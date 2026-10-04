use forge_components::{fmt_short_date, tr};
use forge_runtime::scheduled_runs::{
    ACTION_DISABLED_REASON, ACTION_NOT_FOUND_REASON, ACTIONS_UNAVAILABLE_REASON, MISSED_REASON,
    QUEUES_CLOSED_REASON, UNREADABLE_REASON,
};
use forge_runtime::{
    QUEUE_DRAINING_REASON, QUEUE_NOT_FOUND_REASON, QUEUE_OVERFLOW_REASON, QUEUE_PAUSED_REASON,
};
use forge_storage::{
    ACTION_REMOVED_REASON, CANCELLED_REASON, SUPERSEDED_REASON, ScheduledRunState,
};
use time::{OffsetDateTime, UtcOffset};

const SECS_PER_MINUTE: i64 = 60;
const SECS_PER_HOUR: i64 = 3_600;
const SECS_PER_DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Span {
    Minutes(i64),
    Hours { hours: i64, minutes: i64 },
    Days { days: i64, hours: i64 },
}

impl Span {
    pub(crate) fn of_seconds(seconds: i64) -> Self {
        let seconds = seconds.max(0);
        if seconds < SECS_PER_HOUR {
            Span::Minutes(((seconds + SECS_PER_MINUTE - 1) / SECS_PER_MINUTE).max(1))
        } else if seconds < SECS_PER_DAY {
            Span::Hours {
                hours: seconds / SECS_PER_HOUR,
                minutes: (seconds % SECS_PER_HOUR) / SECS_PER_MINUTE,
            }
        } else {
            Span::Days {
                days: seconds / SECS_PER_DAY,
                hours: (seconds % SECS_PER_DAY) / SECS_PER_HOUR,
            }
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Span::Minutes(minutes) => tr!("scheduled_span_minutes", minutes = minutes),
            Span::Hours { hours, minutes } => {
                tr!("scheduled_span_hours", hours = hours, minutes = minutes)
            }
            Span::Days { days, hours } => tr!("scheduled_span_days", days = days, hours = hours),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Countdown {
    DueNow,
    In(Span),
}

pub(crate) fn countdown(due_at: OffsetDateTime, now: OffsetDateTime) -> Countdown {
    let remaining = (due_at - now).whole_seconds();
    if remaining <= 0 {
        Countdown::DueNow
    } else {
        Countdown::In(Span::of_seconds(remaining))
    }
}

pub(crate) fn countdown_label(countdown: Countdown) -> String {
    match countdown {
        Countdown::DueNow => tr!("queues_scheduled_due_now"),
        Countdown::In(span) => tr!("queues_scheduled_due_in", span = span.label()),
    }
}

pub(crate) fn system_offset_at(at: OffsetDateTime) -> UtcOffset {
    jiff::Timestamp::from_second(at.unix_timestamp())
        .ok()
        .map(|instant| jiff::tz::TimeZone::system().to_offset(instant).seconds())
        .and_then(|seconds| UtcOffset::from_whole_seconds(seconds).ok())
        .unwrap_or(UtcOffset::UTC)
}

pub(crate) fn local_stamp(at: OffsetDateTime, offset: UtcOffset) -> String {
    let local = at.to_offset(offset);
    format!(
        "{} {:02}:{:02}",
        fmt_short_date(&local),
        local.hour(),
        local.minute()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutcomeTone {
    Ran,
    Cancelled,
    Skipped,
    Failed,
}

pub(crate) fn outcome_tone(state: ScheduledRunState) -> OutcomeTone {
    match state {
        ScheduledRunState::Pending | ScheduledRunState::Dispatched => OutcomeTone::Ran,
        ScheduledRunState::Cancelled => OutcomeTone::Cancelled,
        ScheduledRunState::Skipped => OutcomeTone::Skipped,
        ScheduledRunState::Failed => OutcomeTone::Failed,
    }
}

pub(crate) fn outcome_badge_key(tone: OutcomeTone) -> &'static str {
    match tone {
        OutcomeTone::Ran => "queues_scheduled_outcome_ran",
        OutcomeTone::Cancelled => "queues_scheduled_outcome_cancelled",
        OutcomeTone::Skipped => "queues_scheduled_outcome_skipped",
        OutcomeTone::Failed => "queues_scheduled_outcome_failed",
    }
}

pub(crate) fn reason_key(reason: &str) -> Option<&'static str> {
    match reason {
        MISSED_REASON => Some("queues_scheduled_reason_missed"),
        SUPERSEDED_REASON => Some("queues_scheduled_reason_superseded"),
        CANCELLED_REASON => Some("queues_scheduled_reason_cancelled"),
        ACTION_REMOVED_REASON => Some("queues_scheduled_reason_action_removed"),
        ACTION_DISABLED_REASON => Some("queues_scheduled_reason_action_disabled"),
        ACTION_NOT_FOUND_REASON => Some("queues_scheduled_reason_action_not_found"),
        ACTIONS_UNAVAILABLE_REASON => Some("queues_scheduled_reason_actions_unavailable"),
        QUEUES_CLOSED_REASON => Some("queues_scheduled_reason_queues_closed"),
        UNREADABLE_REASON => Some("queues_scheduled_reason_unreadable"),
        QUEUE_PAUSED_REASON => Some("queues_scheduled_reason_queue_paused"),
        QUEUE_DRAINING_REASON => Some("queues_scheduled_reason_queue_draining"),
        QUEUE_OVERFLOW_REASON => Some("queues_scheduled_reason_queue_overflow"),
        QUEUE_NOT_FOUND_REASON => Some("queues_scheduled_reason_queue_not_found"),
        _ => None,
    }
}

pub(crate) fn reason_label(reason: &str, late_by: Option<Span>) -> String {
    match (reason, late_by) {
        (MISSED_REASON, Some(span)) => {
            tr!("queues_scheduled_reason_missed_by", span = span.label())
        }
        _ => reason_key(reason).map_or_else(|| reason.to_owned(), |key| tr!(key)),
    }
}
