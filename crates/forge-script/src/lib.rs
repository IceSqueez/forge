#![doc = "Sandboxed rhai engine wrapper: ForgeApi, Engine, sandbox limits."]

mod api;
mod arg_binding;
mod catalog;
mod coded_text;
mod condition;
mod contract;
mod convert;
mod engine;
mod error;
mod format;
mod globals_port;
mod http_client;
mod http_config;
mod math_evaluator;
mod registration;
mod runner;
mod schedule;
mod schedule_api;
pub mod settings;
#[cfg(test)]
mod test_support;

pub use api::{ForgeApi, ScriptSpeakError, SpeakRequester};
pub use arg_binding::BoundExpression;
pub use catalog::ParamDescriptor;
pub use catalog::{MethodDescriptor, catalog, is_engine_bound_name};
pub use condition::ConditionEvaluator;
pub use contract::InputMismatchError;
pub use contract::{build_scope_for_contract, inert_annotation_lines};
pub use engine::{Engine, EngineConfig, validate_syntax};
pub use error::ScriptError;
pub use format::format_script;
pub use globals_port::{GlobalDeletion, GlobalIncrement, ScriptGlobals, ScriptGlobalsError};
pub use http_client::HttpError;
pub use http_client::HttpResponse;
pub use http_client::ScriptHttpClient;
pub use http_config::{ScriptHttpConfig, stored_script_http_allowed_domains};
pub use math_evaluator::MathEvaluator;
pub use runner::{RunResult, ScriptCapabilities, ScriptHost, ScriptSession, run_inline};
pub use schedule::{
    ActionScheduler, ScriptScheduleDue, ScriptScheduleError, ScriptSchedulePlacement,
    ScriptScheduleRequest,
};
