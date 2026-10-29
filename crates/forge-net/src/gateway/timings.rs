use std::time::Duration;

use super::call_class::CallClass;
use super::streaming::{STREAMING_CONNECT_BUDGET, STREAMING_READ_IDLE_BUDGET};
use crate::rate_limit::MAX_THROTTLE_WAIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GatewayTimings {
    pub interactive_total: Duration,
    pub oauth_total: Duration,
    pub catalog_total: Duration,
    pub media_total: Duration,
    pub streaming_connect: Duration,
    pub streaming_read_idle: Duration,
    pub max_throttle_wait: Duration,
}

impl GatewayTimings {
    pub const DEFAULT: Self = Self {
        interactive_total: CallClass::Interactive.total_budget(),
        oauth_total: CallClass::OAuth.total_budget(),
        catalog_total: CallClass::Catalog.total_budget(),
        media_total: CallClass::Media.total_budget(),
        streaming_connect: STREAMING_CONNECT_BUDGET,
        streaming_read_idle: STREAMING_READ_IDLE_BUDGET,
        max_throttle_wait: MAX_THROTTLE_WAIT,
    };

    pub const fn total_budget(&self, class: CallClass) -> Duration {
        match class {
            CallClass::Interactive => self.interactive_total,
            CallClass::OAuth => self.oauth_total,
            CallClass::Catalog => self.catalog_total,
            CallClass::Media => self.media_total,
        }
    }
}

impl Default for GatewayTimings {
    fn default() -> Self {
        Self::DEFAULT
    }
}
