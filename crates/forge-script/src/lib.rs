#![doc = "Sandboxed rhai engine wrapper: ScriptRegistry, LoomApi, op-count + time limits."]

pub mod engine;
pub mod error;
pub use engine::{Engine, EngineConfig};
pub use error::ScriptError;
