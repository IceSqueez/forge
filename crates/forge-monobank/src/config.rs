use std::time::Duration;

use forge_platform_core::{EndpointSurface, PlatformEndpoints};

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;
const MAX_STATEMENT_WINDOW_DAYS: u64 = 31;
const HISTORY_LOOKBACK_HOURS: u64 = 1;
const WINDOW_OVERLAP_MINUTES: u64 = 5;

pub const STATEMENT_CALL_INTERVAL: Duration = Duration::from_secs(SECONDS_PER_MINUTE);
pub const MAX_STATEMENT_WINDOW: Duration =
    Duration::from_secs(MAX_STATEMENT_WINDOW_DAYS * SECONDS_PER_DAY + SECONDS_PER_HOUR);
pub const DEFAULT_HISTORY_LOOKBACK: Duration =
    Duration::from_secs(HISTORY_LOOKBACK_HOURS * SECONDS_PER_HOUR);
pub const DEFAULT_WINDOW_OVERLAP: Duration =
    Duration::from_secs(WINDOW_OVERLAP_MINUTES * SECONDS_PER_MINUTE);

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonobankConfig {
    pub base_url: String,
    pub request_timeout: Duration,
    pub history_lookback: Duration,
    pub window_overlap: Duration,
}

impl MonobankConfig {
    pub fn new(endpoints: &PlatformEndpoints) -> Self {
        Self {
            base_url: endpoints
                .base_url(EndpointSurface::MonobankApi)
                .trim_end_matches('/')
                .to_owned(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            history_lookback: DEFAULT_HISTORY_LOOKBACK,
            window_overlap: DEFAULT_WINDOW_OVERLAP,
        }
    }
}
