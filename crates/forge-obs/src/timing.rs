use std::time::Duration;

use crate::client::{STATS_POLL_INTERVAL, STATS_POLL_REQUEST_TIMEOUT};
use crate::session::REQUEST_TIMEOUT;

const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
pub struct ObsTiming {
    pub reconnect_base_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub request_timeout: Duration,
    pub connect_timeout: Duration,
    pub stats_poll_interval: Duration,
    pub stats_poll_request_timeout: Duration,
}

impl Default for ObsTiming {
    fn default() -> Self {
        Self {
            reconnect_base_delay: RECONNECT_BASE_DELAY,
            reconnect_max_delay: RECONNECT_MAX_DELAY,
            request_timeout: REQUEST_TIMEOUT,
            connect_timeout: obws::client::DEFAULT_CONNECT_TIMEOUT,
            stats_poll_interval: STATS_POLL_INTERVAL,
            stats_poll_request_timeout: STATS_POLL_REQUEST_TIMEOUT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_timing_keeps_each_production_constant_in_its_own_field() {
        let timing = ObsTiming::default();

        for (label, actual, expected) in [
            (
                "reconnect base",
                timing.reconnect_base_delay,
                RECONNECT_BASE_DELAY,
            ),
            (
                "reconnect max",
                timing.reconnect_max_delay,
                RECONNECT_MAX_DELAY,
            ),
            ("request", timing.request_timeout, REQUEST_TIMEOUT),
            (
                "connect",
                timing.connect_timeout,
                obws::client::DEFAULT_CONNECT_TIMEOUT,
            ),
            (
                "stats interval",
                timing.stats_poll_interval,
                STATS_POLL_INTERVAL,
            ),
            (
                "stats request",
                timing.stats_poll_request_timeout,
                STATS_POLL_REQUEST_TIMEOUT,
            ),
        ] {
            assert_eq!(actual, expected, "{label}");
        }
    }
}
