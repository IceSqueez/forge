use std::sync::Arc;

use forge_events::{Event, EventPublisher};
use forge_platform_core::PlatformError;
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId};
use futures::future::BoxFuture;

pub(crate) struct NoopPublisher;

impl EventPublisher for NoopPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
}

pub(crate) fn token_source()
-> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
    Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
}
