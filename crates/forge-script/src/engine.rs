use std::sync::{Arc, Mutex};
use std::time::Instant;

use rhai::Dynamic;

use crate::ScriptError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub op_limit: u64,
    pub wall_time_ms: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            op_limit: 100_000,
            wall_time_ms: 500,
        }
    }
}

pub struct Engine {
    config: EngineConfig,
    inner: rhai::Engine,
}

impl Engine {
    pub fn with_config(cfg: EngineConfig) -> Self {
        let mut engine = rhai::Engine::new();

        engine.set_max_operations(cfg.op_limit);
        engine.set_max_call_levels(64);
        engine.set_max_expr_depths(64, 32);
        engine.set_max_string_size(1 << 20);
        engine.set_max_array_size(10_000);
        engine.set_max_map_size(10_000);

        let wall_time_ms = cfg.wall_time_ms;
        let start = Arc::new(Mutex::new(Instant::now()));
        let start_clone = Arc::clone(&start);

        engine.on_progress(move |_ops| {
            let elapsed = start_clone
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .elapsed();
            if elapsed.as_millis() > wall_time_ms as u128 {
                Some(Dynamic::UNIT)
            } else {
                None
            }
        });

        Self {
            config: cfg,
            inner: engine,
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Smoke-test only — replaced by LoomApi in alpha-6.
    pub fn placeholder_eval(&self, expr: &str) -> Result<String, ScriptError> {
        self.inner
            .eval::<i64>(expr)
            .map(|n| n.to_string())
            .map_err(|e| map_eval_error(expr, &self.config, *e))
    }
}

fn map_eval_error(script: &str, cfg: &EngineConfig, err: rhai::EvalAltResult) -> ScriptError {
    let reason = err.to_string();
    match err {
        rhai::EvalAltResult::ErrorParsing(..) => ScriptError::Compile {
            script: script.to_owned(),
            reason,
        },
        rhai::EvalAltResult::ErrorTooManyOperations(..) => ScriptError::OperationLimit {
            script: script.to_owned(),
            ops: cfg.op_limit,
        },
        rhai::EvalAltResult::ErrorTerminated(..) => ScriptError::Timeout {
            script: script.to_owned(),
            elapsed_ms: cfg.wall_time_ms,
            limit_ms: cfg.wall_time_ms,
        },
        _ => ScriptError::Runtime {
            script: script.to_owned(),
            reason,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.op_limit, 100_000);
        assert_eq!(cfg.wall_time_ms, 500);
    }

    #[test]
    fn engine_config_accessor_matches() {
        let cfg = EngineConfig::default();
        let engine = Engine::with_config(cfg.clone());
        assert_eq!(engine.config(), &cfg);
    }

    #[test]
    fn placeholder_eval_addition() {
        let engine = Engine::with_config(EngineConfig::default());
        assert_eq!(engine.placeholder_eval("1 + 2").unwrap(), "3");
    }

    #[test]
    fn placeholder_eval_invalid_syntax_returns_compile_error() {
        let engine = Engine::with_config(EngineConfig::default());
        let result = engine.placeholder_eval("not a valid expression");
        assert!(
            matches!(result, Err(ScriptError::Compile { .. })),
            "expected Compile error, got: {result:?}",
        );
    }
}
