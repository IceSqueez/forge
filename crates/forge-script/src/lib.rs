#![doc = "Sandboxed rhai engine wrapper: ForgeApi, Engine, sandbox limits."]

pub mod api;
pub mod catalog;
pub mod contract;
pub mod convert;
pub mod engine;
pub mod error;
pub mod http_config;
pub mod http_deny_list;
pub mod runner;

pub use api::{ForgeApi, SpeakRequester};
pub use catalog::{
    MethodDescriptor, ParamDescriptor, SymbolKind, SymbolToken, catalog, resolve_symbol_from_tokens,
};
pub use contract::{
    ContractParseError, InputMismatchError, build_scope_for_contract, parse_contract,
};
pub use engine::{Engine, EngineConfig, validate_syntax};
pub use error::ScriptError;
pub use http_config::{ScriptHttpConfig, load_script_http_config};
pub use http_deny_list::is_private_or_special;
pub use runner::{RunResult, content_hash, run_inline};
