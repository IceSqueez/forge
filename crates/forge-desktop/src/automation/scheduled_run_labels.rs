use forge_components::tr;
use forge_storage::{RunRefusal, ScheduledRunState};
use time::OffsetDateTime;

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
        let minutes_per_hour = SECS_PER_HOUR / SECS_PER_MINUTE;
        let rounded_up_minutes = (seconds + SECS_PER_MINUTE - 1) / SECS_PER_MINUTE;
        if rounded_up_minutes < minutes_per_hour {
            Span::Minutes(rounded_up_minutes.max(1))
        } else if seconds < SECS_PER_HOUR {
            Span::Hours {
                hours: 1,
                minutes: 0,
            }
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

pub(crate) fn reason_key(reason: RunRefusal) -> &'static str {
    match reason {
        RunRefusal::Missed => "queues_scheduled_reason_missed",
        RunRefusal::Superseded => "queues_scheduled_reason_superseded",
        RunRefusal::Cancelled => "queues_scheduled_reason_cancelled",
        RunRefusal::ActionRemoved => "queues_scheduled_reason_action_removed",
        RunRefusal::ActionDisabled => "queues_scheduled_reason_action_disabled",
        RunRefusal::ActionNotFound => "queues_scheduled_reason_action_not_found",
        RunRefusal::ActionsUnavailable => "queues_scheduled_reason_actions_unavailable",
        RunRefusal::QueuesClosed => "queues_scheduled_reason_queues_closed",
        RunRefusal::Unreadable => "queues_scheduled_reason_unreadable",
        RunRefusal::QueuePaused => "queues_scheduled_reason_queue_paused",
        RunRefusal::QueueDraining => "queues_scheduled_reason_queue_draining",
        RunRefusal::QueueOverflow => "queues_scheduled_reason_queue_overflow",
        RunRefusal::QueueNotFound => "queues_scheduled_reason_queue_not_found",
        RunRefusal::QueueDeleted => "queues_scheduled_reason_queue_deleted",
        RunRefusal::EngineStopped => "queues_scheduled_reason_engine_stopped",
    }
}

pub(crate) fn reason_label(reason: RunRefusal, late_by: Option<Span>) -> String {
    match (reason, late_by) {
        (RunRefusal::Missed, Some(span)) => {
            tr!("queues_scheduled_reason_missed_by", span = span.label())
        }
        _ => tr!(reason_key(reason)),
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use super::*;
    use crate::app::i18n::{install_language, message_in};

    const OCT_4_2026_NOON_UTC: i64 = 1_791_115_200;

    fn plain(rendered: String) -> String {
        rendered.replace(crate::test_support::FORMAT_ISOLATES, "")
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
                3_541,
                Span::Hours {
                    hours: 1,
                    minutes: 0,
                },
            ),
            (
                3_599,
                Span::Hours {
                    hours: 1,
                    minutes: 0,
                },
            ),
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
    fn every_run_refusal_has_a_label_in_every_locale() {
        for reason in RunRefusal::ALL {
            let key = reason_key(reason);
            for language in [Language::En, Language::Uk] {
                assert_ne!(
                    message_in(language, key),
                    key,
                    "{language:?} catalog is missing {key}"
                );
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
                RunRefusal::Missed,
                late,
                "Missed by 1 h 30 min - forge was not running at the due time",
            ),
            (
                Language::En,
                RunRefusal::Missed,
                None,
                "Missed - forge was not running at the due time",
            ),
            (
                Language::En,
                RunRefusal::Cancelled,
                late,
                "Cancelled before it was due",
            ),
            (
                Language::Uk,
                RunRefusal::Missed,
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
}
