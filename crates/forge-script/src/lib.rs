#![doc = "Sandboxed rhai engine wrapper: ForgeApi, Engine, sandbox limits."]

pub mod api;
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
pub mod user_functions;

// requires manual bump when rhai version changes in Cargo.toml
pub const RHAI_VERSION: &str = "1.25";

pub use api::{ForgeApi, SpeakRequester};
pub use catalog::{
    MethodDescriptor, ParamDescriptor, SymbolKind, SymbolToken, catalog, resolve_symbol_from_tokens,
};
pub use condition::ConditionEvaluator;
pub use contract::{
    ContractParseError, InputMismatchError, build_scope_for_contract, parse_contract,
};
pub use engine::{Engine, EngineConfig, validate_syntax};
pub use error::ScriptError;
pub use forge_platform_core::is_private_or_special;
pub use format::format_script;
pub use http_client::{HttpError, HttpResponse, ScriptHttpClient};
pub use http_config::{ScriptHttpConfig, load_script_http_config};
pub use math_evaluator::MathEvaluator;
pub use runner::{RunResult, content_hash, run_inline};
pub use user_functions::{UserFunctionSig, UserParam, collect_user_functions};
