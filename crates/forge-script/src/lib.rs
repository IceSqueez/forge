#![doc = "Sandboxed rhai engine wrapper: ForgeApi, Engine, sandbox limits."]

pub mod api;
pub mod arg_binding;
pub mod catalog;
pub mod condition;
pub mod contract;
pub mod convert;
pub mod engine;
pub mod error;
pub mod format;
pub mod http_client;
pub mod http_config;
pub mod math_evaluator;
pub mod runner;
pub mod schedule;
mod schedule_api;
#[cfg(test)]
mod test_support;

pub use api::{
    ENGINE_BOUND_NAMES, ForgeApi, ScriptSpeakError, SpeakRequester, is_engine_bound_name,
};
pub use arg_binding::BoundExpression;
pub use catalog::{MethodDescriptor, ParamDescriptor, catalog};
pub use condition::ConditionEvaluator;
pub use contract::{InputMismatchError, build_scope_for_contract, inert_annotation_lines};
pub use engine::{Engine, EngineConfig, load_script_engine_config, validate_syntax};
pub use error::ScriptError;
pub use format::format_script;
pub use http_client::{HttpError, HttpResponse, ScriptHttpClient};
pub use http_config::{ScriptHttpConfig, load_script_http_config};
pub use math_evaluator::MathEvaluator;
pub use runner::{RunResult, ScriptHost, content_hash, run_inline};
pub use schedule::{
    ActionScheduler, ScriptScheduleDue, ScriptScheduleError, ScriptSchedulePlacement,
    ScriptScheduleRequest,
};
pub use schedule_api::{
    PLACEMENT_DUE_AT_FIELD, PLACEMENT_ID_FIELD, SCHEDULE_INHERIT_ARGS_OPTION, SCHEDULE_KEY_OPTION,
    SCHEDULE_SKIP_IF_LATE_MINUTES_OPTION, SCHEDULING_UNAVAILABLE_REASON,
};
