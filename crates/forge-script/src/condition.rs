use std::time::{Duration, Instant};

use rhai::Dynamic;

use crate::ScriptError;
use crate::engine::{EngineConfig, map_eval_error, register_sandbox_base};

/// Bounded boolean evaluator for predicate-primitive conditions. It carries no
/// `ForgeApi`: conditions are side-effect-free, so the globals/chat/http surface
/// is structurally unreachable from a condition rather than merely unused.
#[derive(Debug, Clone)]
pub struct ConditionEvaluator {
    config: EngineConfig,
}

impl ConditionEvaluator {
    pub fn with_config(config: EngineConfig) -> Self {
        Self { config }
    }

    /// Accepts only a genuine boolean result; any other type is a typed error,
    /// never truthiness-coerced — truthiness across the seven Variant kinds is
    /// ambiguous and would silently mask authoring mistakes.
    pub fn eval(&self, expr: &str) -> Result<bool, ScriptError> {
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

        value
            .as_bool()
            .map_err(|got| ScriptError::ConditionNotBoolean {
                expr: expr.to_owned(),
                got: got.to_owned(),
            })
    }
}
