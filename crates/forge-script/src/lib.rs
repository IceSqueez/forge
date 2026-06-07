#![doc = "Sandboxed rhai engine wrapper: ForgeApi, Engine, sandbox limits."]

pub mod api;
pub mod catalog;
pub mod contract;
pub mod convert;
pub mod engine;
pub mod error;
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
pub use runner::{RunResult, content_hash, run_inline};
