use std::sync::Arc;

use forge_events::test_support::NullPublisher;
use forge_platform_core::PlatformError;
use forge_registry::RunContext;
use forge_types::{ArgStack, EventId, MessageArg, UserText};
use futures::future::BoxFuture;

pub(crate) fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
    RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
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

pub(crate) fn broadcaster_id_source(
    id: u64,
) -> Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync> {
    Arc::new(move || Box::pin(async move { Ok(id) }))
}
