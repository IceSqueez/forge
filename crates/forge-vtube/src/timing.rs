use std::time::Duration;

use crate::supervisor::{
    EXPRESSION_POLL_INTERVAL, EXPRESSION_POLL_TIMEOUT, HANDSHAKE_REPLY_TIMEOUT,
    REQUEST_SWEEP_INTERVAL, TOKEN_APPROVAL_TIMEOUT,
};
use crate::vts::request::REQUEST_TIMEOUT;

#[derive(Clone, Copy, Debug)]
pub struct VTubeTiming {
    pub request_timeout: Duration,
    pub handshake_reply_timeout: Duration,
    pub token_approval_timeout: Duration,
    pub expression_poll_interval: Duration,
    pub expression_poll_timeout: Duration,
}

impl Default for VTubeTiming {
    fn default() -> Self {
        Self {
            request_timeout: REQUEST_TIMEOUT,
            handshake_reply_timeout: HANDSHAKE_REPLY_TIMEOUT,
            token_approval_timeout: TOKEN_APPROVAL_TIMEOUT,
            expression_poll_interval: EXPRESSION_POLL_INTERVAL,
            expression_poll_timeout: EXPRESSION_POLL_TIMEOUT,
        }
    }
}

impl VTubeTiming {
    pub(crate) fn unresponsive_threshold(&self) -> Duration {
        self.request_timeout.saturating_add(REQUEST_SWEEP_INTERVAL)
    }
}
