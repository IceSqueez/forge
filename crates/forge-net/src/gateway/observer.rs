use super::call_class::CallClass;
use super::failure::GatewayFailure;
use super::request::Method;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureContext {
    pub method: Method,
    pub class: CallClass,
    pub label: Option<&'static str>,
}

pub trait FailureObserver: Send + Sync {
    fn on_failure(&self, failure: &GatewayFailure, context: FailureContext);
}
