use std::sync::Arc;

use forge_events::{Event, EventPublisher};
use forge_platform_core::PlatformError;
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, MessageArg, UserText};
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

pub(crate) fn rejected_field(text: &UserText) -> Option<&str> {
    let UserText::Message(message) = text else {
        return None;
    };
    match message.arg("field") {
        Some(MessageArg::StepFields { keys, .. }) if keys.len() == 1 => Some(keys[0].as_str()),
        _ => None,
    }
}

pub(crate) fn step_problem(kind: &str, problem: &forge_registry::ConfigProblem) -> UserText {
    problem
        .message(|keys| MessageArg::StepFields {
            kind: kind.to_owned(),
            keys: keys.to_vec(),
        })
        .into()
}
