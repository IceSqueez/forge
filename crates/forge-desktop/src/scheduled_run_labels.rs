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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::path::Path;

    use forge_storage::Language;

    use super::*;
    use crate::i18n::{install_language, message_in};

    const OCT_4_2026_NOON_UTC: i64 = 1_791_115_200;
    const MAR_15_2026_2330_UTC: i64 = 1_773_617_400;

    const REASON_SOURCES: [&str; 3] = [
        "../forge-storage/src/scheduled_run.rs",
        "../forge-runtime/src/scheduled_runs/hand_off.rs",
        "../forge-runtime/src/queue_scheduler.rs",
    ];

    fn plain(rendered: String) -> String {
        rendered
            .chars()
            .filter(|c| !matches!(c, '\u{2068}' | '\u{2069}'))
            .collect()
    }

    fn reason_tokens_declared_in(source: &str) -> Vec<String> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(source);
        std::fs::read_to_string(&path)
            .unwrap()
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub const "))
            .filter_map(|rest| rest.split_once("_REASON: &str = \""))
            .filter_map(|(_, value)| value.strip_suffix("\";"))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn span_of_seconds_rounds_up_to_minutes_then_floors_hours_and_days() {
        for (seconds, expected) in [
            (-30, Span::Minutes(1)),
            (0, Span::Minutes(1)),
            (1, Span::Minutes(1)),
            (59, Span::Minutes(1)),
            (60, Span::Minutes(1)),
            (61, Span::Minutes(2)),
            (3_540, Span::Minutes(59)),
            (
                3_600,
                Span::Hours {
                    hours: 1,
                    minutes: 0,
                },
            ),
            (
                3_661,
                Span::Hours {
                    hours: 1,
                    minutes: 1,
                },
            ),
            (
                86_399,
                Span::Hours {
                    hours: 23,
                    minutes: 59,
                },
            ),
            (86_400, Span::Days { days: 1, hours: 0 }),
            (90_061, Span::Days { days: 1, hours: 1 }),
        ] {
            assert_eq!(Span::of_seconds(seconds), expected, "{seconds} s");
        }
    }

    #[test]
    fn countdown_is_due_now_from_the_due_second_onwards() {
        let now = OffsetDateTime::from_unix_timestamp(OCT_4_2026_NOON_UTC).unwrap();
        for (due_in_secs, expected) in [
            (-172_800, Countdown::DueNow),
            (-1, Countdown::DueNow),
            (0, Countdown::DueNow),
            (1, Countdown::In(Span::Minutes(1))),
            (
                3_600,
                Countdown::In(Span::Hours {
                    hours: 1,
                    minutes: 0,
                }),
            ),
        ] {
            let due = now + time::Duration::seconds(due_in_secs);
            assert_eq!(countdown(due, now), expected, "due in {due_in_secs} s");
        }
    }

    #[test]
    fn countdown_label_renders_due_now_or_the_remaining_span() {
        let in_two_hours = Countdown::In(Span::Hours {
            hours: 2,
            minutes: 5,
        });
        for (language, remaining, expected) in [
            (Language::En, Countdown::DueNow, "due now"),
            (Language::En, in_two_hours, "in 2 h 5 min"),
            (Language::Uk, Countdown::DueNow, "час настав"),
            (Language::Uk, in_two_hours, "через 2 год 5 хв"),
        ] {
            install_language(language);
            assert_eq!(plain(countdown_label(remaining)), expected, "{language:?}");
        }
    }

    #[test]
    fn every_reason_token_storage_or_runtime_declares_has_a_label_in_every_locale() {
        for source in REASON_SOURCES {
            let tokens = reason_tokens_declared_in(source);
            assert!(!tokens.is_empty(), "{source} declares no reason tokens");
            for token in tokens {
                let key = reason_key(&token)
                    .unwrap_or_else(|| panic!("{token} from {source} has no label key"));
                for language in [Language::En, Language::Uk] {
                    assert_ne!(
                        message_in(language, key),
                        key,
                        "{language:?} catalog is missing {key}"
                    );
                }
            }
        }
    }

    #[test]
    fn reason_label_says_missed_by_the_late_span_only_for_missed_runs() {
        let late = Some(Span::Hours {
            hours: 1,
            minutes: 30,
        });
        for (language, reason, late_by, expected) in [
            (
                Language::En,
                MISSED_REASON,
                late,
                "Missed by 1 h 30 min - forge was not running at the due time",
            ),
            (
                Language::En,
                MISSED_REASON,
                None,
                "Missed - forge was not running at the due time",
            ),
            (
                Language::En,
                CANCELLED_REASON,
                late,
                "Cancelled before it was due",
            ),
            (
                Language::Uk,
                MISSED_REASON,
                late,
                "Пропущено на 1 год 30 хв - forge не працював у призначений час",
            ),
        ] {
            install_language(language);
            assert_eq!(
                plain(reason_label(reason, late_by)),
                expected,
                "{language:?} {reason}"
            );
        }
    }

    #[test]
    fn reason_label_shows_an_unknown_reason_verbatim() {
        install_language(Language::En);
        assert_eq!(reason_label("vendor_glitch", None), "vendor_glitch");
    }

    #[test]
    fn every_outcome_badge_has_a_label_in_every_locale() {
        for state in [
            ScheduledRunState::Pending,
            ScheduledRunState::Dispatched,
            ScheduledRunState::Cancelled,
            ScheduledRunState::Skipped,
            ScheduledRunState::Failed,
        ] {
            let key = outcome_badge_key(outcome_tone(state));
            for language in [Language::En, Language::Uk] {
                assert_ne!(message_in(language, key), key, "{language:?} {state:?}");
            }
        }
    }

    #[test]
    fn local_stamp_shifts_the_instant_into_the_offset_across_midnight() {
        install_language(Language::En);
        assert_eq!(
            local_stamp(
                OffsetDateTime::from_unix_timestamp(MAR_15_2026_2330_UTC).unwrap(),
                UtcOffset::from_hms(3, 0, 0).unwrap()
            ),
            "Mar 16, 2026 02:30"
        );
    }
}
