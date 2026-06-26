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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn eval(expr: &str) -> Result<bool, ScriptError> {
        ConditionEvaluator::with_config(EngineConfig::default()).eval(expr)
    }

    #[test]
    fn eval_returns_the_boolean_each_comparison_operator_computes() {
        // Covers every comparison/equality operator across the primitive kinds,
        // with the expected verdict reasoned independently of rhai.
        for (expr, expected) in [
            ("5 > 3", true),
            ("5 < 3", false),
            ("5 <= 5", true),
            ("6 >= 7", false),
            ("3 != 4", true),
            ("4 != 4", false),
            ("true == true", true),
            ("true == false", false),
            (r#""hi" == "hi""#, true),
            (r#""hi" == "bye""#, false),
        ] {
            assert_eq!(eval(expr).unwrap(), expected, "expr: {expr}");
        }
    }

    #[test]
    fn eval_rejects_non_boolean_result_instead_of_coercing_truthiness() {
        // A condition must be a genuine boolean; ints/floats/strings are a typed
        // authoring error, never silently truthy.
        for expr in ["1 + 1", "42", "3.14", r#""x""#] {
            let err = eval(expr).unwrap_err();
            assert!(
                matches!(err, ScriptError::ConditionNotBoolean { .. }),
                "expr {expr} expected ConditionNotBoolean, got {err:?}",
            );
        }
    }

    #[test]
    fn eval_carries_the_offending_expression_in_the_coercion_error() {
        let err = eval("7").unwrap_err();
        match err {
            ScriptError::ConditionNotBoolean { expr, .. } => assert_eq!(expr, "7"),
            other => panic!("expected ConditionNotBoolean, got {other:?}"),
        }
    }

    #[test]
    fn eval_exceeding_op_limit_returns_typed_error_not_hang() {
        // Tiny op budget, generous wall budget: the loop must trip the op limit.
        let evaluator = ConditionEvaluator::with_config(EngineConfig {
            op_limit: 100,
            wall_time_ms: 5_000,
        });
        let err = evaluator
            .eval("let x = 0; while x < 1000000 { x += 1; } x > 0")
            .unwrap_err();
        assert!(
            matches!(err, ScriptError::OperationLimit { .. }),
            "expected OperationLimit, got {err:?}",
        );
    }

    #[test]
    fn eval_exceeding_wall_budget_returns_typed_error_without_hanging() {
        // Huge op budget, 1ms wall budget: the infinite loop must trip on time.
        let evaluator = ConditionEvaluator::with_config(EngineConfig {
            op_limit: 1_000_000_000,
            wall_time_ms: 1,
        });
        let start = Instant::now();
        let err = evaluator.eval("loop {}").unwrap_err();
        let elapsed = start.elapsed();
        assert!(
            matches!(err, ScriptError::Timeout { .. }),
            "expected Timeout, got {err:?}",
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "wall-bounded eval must return promptly, took {elapsed:?}",
        );
    }
}
