use std::time::Duration;

use crate::probe::PROBE_TIMEOUT;
use crate::supervisor::{
    CONNECT_TIMEOUT, EXPRESSION_POLL_INTERVAL, EXPRESSION_POLL_TIMEOUT, HANDSHAKE_REPLY_TIMEOUT,
    RECONNECT_BASE_DELAY, RECONNECT_MAX_DELAY, REQUEST_SWEEP_INTERVAL, TOKEN_APPROVAL_TIMEOUT,
};
use crate::vts::request::REQUEST_TIMEOUT;

#[derive(Clone, Copy, Debug)]
pub struct VTubeTiming {
    pub reconnect_base_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub connect_timeout: Duration,
    pub probe_timeout: Duration,
    pub request_timeout: Duration,
    pub request_sweep_interval: Duration,
    pub handshake_reply_timeout: Duration,
    pub token_approval_timeout: Duration,
    pub expression_poll_interval: Duration,
    pub expression_poll_timeout: Duration,
}

impl Default for VTubeTiming {
    fn default() -> Self {
        Self {
            reconnect_base_delay: RECONNECT_BASE_DELAY,
            reconnect_max_delay: RECONNECT_MAX_DELAY,
            connect_timeout: CONNECT_TIMEOUT,
            probe_timeout: PROBE_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            request_sweep_interval: REQUEST_SWEEP_INTERVAL,
            handshake_reply_timeout: HANDSHAKE_REPLY_TIMEOUT,
            token_approval_timeout: TOKEN_APPROVAL_TIMEOUT,
            expression_poll_interval: EXPRESSION_POLL_INTERVAL,
            expression_poll_timeout: EXPRESSION_POLL_TIMEOUT,
        }
    }
}

impl VTubeTiming {
    pub(crate) fn unresponsive_threshold(&self) -> Duration {
        self.request_timeout
            .saturating_add(self.request_sweep_interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_timing_keeps_each_production_constant_in_its_own_field() {
        let timing = VTubeTiming::default();

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
            ("connect", timing.connect_timeout, CONNECT_TIMEOUT),
            ("probe", timing.probe_timeout, PROBE_TIMEOUT),
            ("request", timing.request_timeout, REQUEST_TIMEOUT),
            (
                "request sweep",
                timing.request_sweep_interval,
                REQUEST_SWEEP_INTERVAL,
            ),
            (
                "handshake reply",
                timing.handshake_reply_timeout,
                HANDSHAKE_REPLY_TIMEOUT,
            ),
            (
                "token approval",
                timing.token_approval_timeout,
                TOKEN_APPROVAL_TIMEOUT,
            ),
            (
                "expression poll interval",
                timing.expression_poll_interval,
                EXPRESSION_POLL_INTERVAL,
            ),
            (
                "expression poll timeout",
                timing.expression_poll_timeout,
                EXPRESSION_POLL_TIMEOUT,
            ),
        ] {
            assert_eq!(actual, expected, "{label}");
        }
    }

    #[test]
    fn the_unresponsive_threshold_adds_one_sweep_and_saturates() {
        for (request_timeout, sweep, expected) in [
            (
                Duration::from_millis(100),
                Duration::from_millis(7),
                Duration::from_millis(107),
            ),
            (Duration::MAX, Duration::from_secs(1), Duration::MAX),
        ] {
            let timing = VTubeTiming {
                request_timeout,
                request_sweep_interval: sweep,
                ..VTubeTiming::default()
            };

            assert_eq!(timing.unresponsive_threshold(), expected);
        }
    }
}
