use std::time::{Duration, Instant};

use forge_types::Variant;
use rhai::Dynamic;

use crate::ScriptError;
use crate::engine::{EngineConfig, map_eval_error, register_sandbox_base};

/// Bounded arithmetic evaluator. Carries no `ForgeApi`: math expressions are
/// side-effect-free; the globals/chat/http surface is structurally unreachable.
#[derive(Debug, Clone)]
pub struct MathEvaluator {
    config: EngineConfig,
}

impl MathEvaluator {
    pub fn with_config(config: EngineConfig) -> Self {
        Self { config }
    }

    /// Returns `Variant::Int` or `Variant::Float`. NaN and infinite results
    /// are rejected — the caller must not assume any special-float semantics
    /// survive into ArgStack or downstream sub-actions.
    pub fn eval(&self, expr: &str) -> Result<Variant, ScriptError> {
        let mut inner = rhai::Engine::new_raw();
        register_sandbox_base(&mut inner, &self.config);

        let deadline = Instant::now() + Duration::from_millis(self.config.wall_time_ms);
        inner.on_progress(move |_ops| {
            if Instant::now() >= deadline {
                Some(Dynamic::UNIT)
            } else {
                None
            }
        });

        let mut scope = rhai::Scope::new();
        let value = inner
            .eval_with_scope::<Dynamic>(&mut scope, expr)
            .map_err(|e| map_eval_error(expr, &self.config, *e))?;

        if let Ok(n) = value.as_int() {
            Ok(Variant::Int(n))
        } else if let Ok(f) = value.as_float() {
            if f.is_nan() || f.is_infinite() {
                Err(ScriptError::Runtime {
                    script: expr.to_owned(),
                    reason: "non-finite result".to_owned(),
                })
            } else {
                Ok(Variant::Float(f))
            }
        } else {
            Err(ScriptError::Runtime {
                script: expr.to_owned(),
                reason: format!("expected numeric result, got {}", value.type_name()),
            })
        }
    }
}
