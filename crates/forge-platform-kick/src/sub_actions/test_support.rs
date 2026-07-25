use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter};
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId};
use futures::future::BoxFuture;

pub(crate) struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) struct GrantLimiter;

#[async_trait]
impl RateLimiter for GrantLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        120
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}

pub(crate) fn token_source()
-> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
    Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
}
