use std::time::Duration;

use forge_platform_core::{EndpointSurface, PlatformEndpoints};

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(15);
pub const MIN_POLL_INTERVAL: Duration = Duration::from_secs(10);
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(300);
const ERROR_BACKOFF_BASE: Duration = Duration::from_secs(5);
const ERROR_BACKOFF_CAP: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug)]
pub struct DonatelloTiming {
    pub min_poll_interval: Duration,
    pub error_backoff_base: Duration,
    pub error_backoff_cap: Duration,
}

impl Default for DonatelloTiming {
    fn default() -> Self {
        Self {
            min_poll_interval: MIN_POLL_INTERVAL,
            error_backoff_base: ERROR_BACKOFF_BASE,
            error_backoff_cap: ERROR_BACKOFF_CAP,
        }
    }
}

impl DonatelloTiming {
    pub(crate) fn bounded_poll_interval(&self, requested: Duration) -> Duration {
        requested.max(self.min_poll_interval).min(MAX_POLL_INTERVAL)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonatelloConfig {
    pub base_url: String,
    pub poll_interval: Duration,
}

impl DonatelloConfig {
    pub fn new(endpoints: &PlatformEndpoints) -> Self {
        Self {
            base_url: endpoints
                .base_url(EndpointSurface::DonatelloApi)
                .trim_end_matches('/')
                .to_owned(),
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }
}
