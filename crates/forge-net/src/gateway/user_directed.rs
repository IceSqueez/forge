use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::Gateway;
use super::profile::GatewayProfile;
use super::target::GatewayBuildError;
use crate::rate_limit::{RateLimitError, RateLimitOutcome, RateLimiter};

struct UnmeteredLimiter;

#[async_trait]
impl RateLimiter for UnmeteredLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
        Ok(RateLimitOutcome::Granted)
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

impl Gateway {
    pub fn user_directed(allow_local: bool) -> Result<Self, GatewayBuildError> {
        Self::without_base(
            Arc::new(UnmeteredLimiter),
            GatewayProfile::user_directed(allow_local),
            None,
        )
    }
}
