use std::time::Duration;

use forge_platform_core::{EndpointSurface, PlatformEndpoints};

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(15);
pub const MIN_POLL_INTERVAL: Duration = Duration::from_secs(10);
pub const MAX_POLL_INTERVAL: Duration = Duration::from_secs(300);

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonatelloConfig {
    pub base_url: String,
    pub poll_interval: Duration,
    pub request_timeout: Duration,
}

impl DonatelloConfig {
    pub fn new(endpoints: &PlatformEndpoints) -> Self {
        Self {
            base_url: endpoints
                .base_url(EndpointSurface::DonatelloApi)
                .trim_end_matches('/')
                .to_owned(),
            poll_interval: DEFAULT_POLL_INTERVAL,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

pub(crate) fn bounded_poll_interval(requested: Duration) -> Duration {
    requested.clamp(MIN_POLL_INTERVAL, MAX_POLL_INTERVAL)
}
