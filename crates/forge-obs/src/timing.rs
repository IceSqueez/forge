use std::time::Duration;

use crate::session::REQUEST_TIMEOUT;

const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
pub struct ObsTiming {
    pub reconnect_base_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub request_timeout: Duration,
    pub connect_timeout: Duration,
}

impl Default for ObsTiming {
    fn default() -> Self {
        Self {
            reconnect_base_delay: RECONNECT_BASE_DELAY,
            reconnect_max_delay: RECONNECT_MAX_DELAY,
            request_timeout: REQUEST_TIMEOUT,
            connect_timeout: obws::client::DEFAULT_CONNECT_TIMEOUT,
        }
    }
}
