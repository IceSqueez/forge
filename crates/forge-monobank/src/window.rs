use std::time::Duration;

use forge_types::DonationOrigin;

use crate::config::MAX_STATEMENT_WINDOW;

pub(crate) const STATEMENT_PAGE_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatementWindow {
    pub(crate) from_unix: i64,
    pub(crate) to_unix: i64,
    pub(crate) origin: DonationOrigin,
    pub(crate) advances_coverage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowPlanner {
    pub(crate) history_lookback: Duration,
    pub(crate) overlap: Duration,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Coverage {
    covered_until: Option<i64>,
    pending_older_page: Option<StatementWindow>,
}

impl WindowPlanner {
    pub(crate) fn next(&self, coverage: &Coverage, now_unix: i64) -> StatementWindow {
        if let Some(older_page) = coverage.pending_older_page {
            return older_page;
        }
        let (start, origin) = match coverage.covered_until {
            None => (
                now_unix.saturating_sub(seconds(self.history_lookback)),
                DonationOrigin::History,
            ),
            Some(covered) => (
                covered.saturating_sub(seconds(self.overlap)),
                DonationOrigin::Live,
            ),
        };
        let earliest = now_unix.saturating_sub(seconds(MAX_STATEMENT_WINDOW));
        StatementWindow {
            from_unix: start.max(earliest).min(now_unix),
            to_unix: now_unix,
            origin,
            advances_coverage: true,
        }
    }
}

impl Coverage {
    pub(crate) fn record(
        &mut self,
        window: StatementWindow,
        item_count: usize,
        oldest_item_unix: Option<i64>,
    ) {
        if window.advances_coverage {
            self.covered_until = Some(window.to_unix);
        }
        self.pending_older_page = oldest_item_unix
            .filter(|oldest| {
                item_count >= STATEMENT_PAGE_LIMIT
                    && *oldest > window.from_unix
                    && *oldest < window.to_unix
            })
            .map(|oldest| StatementWindow {
                from_unix: window.from_unix,
                to_unix: oldest,
                origin: window.origin,
                advances_coverage: false,
            });
    }
}

fn seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use forge_types::DonationOrigin;

    use super::{Coverage, STATEMENT_PAGE_LIMIT, StatementWindow, WindowPlanner};

    const NOW: i64 = 1_791_000_000;
    const HOUR: i64 = 3_600;
    const DAY: i64 = 24 * HOUR;
    const LONGEST_WINDOW_SECS: i64 = 31 * DAY + HOUR;

    fn planner() -> WindowPlanner {
        WindowPlanner {
            history_lookback: Duration::from_secs(3_600),
            overlap: Duration::from_secs(300),
        }
    }

    fn covered_until(until: i64) -> Coverage {
        let mut coverage = Coverage::default();
        coverage.record(
            StatementWindow {
                from_unix: until - HOUR,
                to_unix: until,
                origin: DonationOrigin::History,
                advances_coverage: true,
            },
            0,
            None,
        );
        coverage
    }

    #[test]
    fn first_window_looks_back_over_the_history_horizon() {
        let window = planner().next(&Coverage::default(), NOW);
        assert_eq!(
            window,
            StatementWindow {
                from_unix: NOW - HOUR,
                to_unix: NOW,
                origin: DonationOrigin::History,
                advances_coverage: true,
            }
        );
    }

    #[test]
    fn later_windows_are_live_and_overlap_the_covered_edge() {
        let window = planner().next(&covered_until(NOW - 60), NOW);
        assert_eq!(
            (window.from_unix, window.to_unix, window.origin),
            (NOW - 60 - 300, NOW, DonationOrigin::Live)
        );
    }

    #[test]
    fn window_never_spans_more_than_the_bank_allows() {
        let second = Duration::from_secs(1);
        let longest = Duration::from_secs(LONGEST_WINDOW_SECS.unsigned_abs());
        for (lookback, from) in [
            (longest - second, NOW - LONGEST_WINDOW_SECS + 1),
            (longest, NOW - LONGEST_WINDOW_SECS),
            (longest + second, NOW - LONGEST_WINDOW_SECS),
            (Duration::from_secs(40 * 86_400), NOW - LONGEST_WINDOW_SECS),
            (Duration::MAX, NOW - LONGEST_WINDOW_SECS),
        ] {
            let planner = WindowPlanner {
                history_lookback: lookback,
                ..planner()
            };
            assert_eq!(
                planner.next(&Coverage::default(), NOW).from_unix,
                from,
                "{lookback:?}"
            );
        }
    }

    #[test]
    fn live_window_after_a_long_idle_is_clamped_to_the_longest_window() {
        let window = planner().next(&covered_until(NOW - 40 * DAY), NOW);
        assert_eq!(window.from_unix, NOW - LONGEST_WINDOW_SECS);
    }

    #[test]
    fn covered_edge_ahead_of_the_clock_never_yields_an_inverted_window() {
        let window = planner().next(&covered_until(NOW + HOUR), NOW);
        assert_eq!((window.from_unix, window.to_unix), (NOW, NOW));
    }

    #[test]
    fn full_page_schedules_the_older_remainder_ending_at_its_oldest_item() {
        let planner = planner();
        let mut coverage = Coverage::default();
        let first = planner.next(&coverage, NOW);
        coverage.record(first, STATEMENT_PAGE_LIMIT, Some(NOW - 600));

        assert_eq!(
            planner.next(&coverage, NOW + 60),
            StatementWindow {
                from_unix: first.from_unix,
                to_unix: NOW - 600,
                origin: DonationOrigin::History,
                advances_coverage: false,
            }
        );
    }

    #[test]
    fn page_below_the_limit_schedules_no_older_page() {
        let planner = planner();
        let mut coverage = Coverage::default();
        let first = planner.next(&coverage, NOW);
        coverage.record(first, STATEMENT_PAGE_LIMIT - 1, Some(NOW - 600));

        assert_eq!(
            planner.next(&coverage, NOW + 60).origin,
            DonationOrigin::Live
        );
    }

    #[test]
    fn full_page_whose_oldest_item_sits_on_a_window_edge_stops_paging() {
        let planner = planner();
        for oldest in [NOW - HOUR, NOW] {
            let mut coverage = Coverage::default();
            let first = planner.next(&coverage, NOW);
            coverage.record(first, STATEMENT_PAGE_LIMIT, Some(oldest));
            assert!(
                planner.next(&coverage, NOW + 60).advances_coverage,
                "oldest {oldest}"
            );
        }
    }

    #[test]
    fn backfill_pages_strictly_lower_the_upper_bound_then_resume_live_from_the_covered_edge() {
        let planner = planner();
        let mut coverage = Coverage::default();
        let mut window = planner.next(&coverage, NOW);
        let mut uppers = vec![window.to_unix];
        for oldest in [NOW - 600, NOW - 1_200, NOW - 1_800] {
            coverage.record(window, STATEMENT_PAGE_LIMIT, Some(oldest));
            window = planner.next(&coverage, NOW + 60);
            uppers.push(window.to_unix);
        }
        coverage.record(window, 10, Some(NOW - 3_000));
        let resumed = planner.next(&coverage, NOW + 120);

        assert_eq!(uppers, [NOW, NOW - 600, NOW - 1_200, NOW - 1_800]);
        assert_eq!(
            (resumed.from_unix, resumed.origin),
            (NOW - 300, DonationOrigin::Live)
        );
    }
}
