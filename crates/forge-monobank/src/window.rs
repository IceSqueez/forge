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
