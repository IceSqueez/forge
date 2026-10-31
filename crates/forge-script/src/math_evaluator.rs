use std::time::{Duration, Instant};

use forge_types::Variant;
use rhai::Dynamic;

use crate::ScriptError;
use crate::arg_binding::BoundExpression;
use crate::engine::{EngineConfig, map_eval_error, register_sandbox_base};

#[derive(Debug, Clone)]
pub struct MathEvaluator {
    config: EngineConfig,
}

impl MathEvaluator {
    pub fn with_config(config: EngineConfig) -> Self {
        Self { config }
    }

    pub fn eval(&self, expr: &str) -> Result<Variant, ScriptError> {
        self.run(expr, rhai::Scope::new())
    }

    pub fn eval_bound(&self, expr: &BoundExpression) -> Result<Variant, ScriptError> {
        self.run(expr.source(), expr.scope())
    }

    fn run(&self, source: &str, mut scope: rhai::Scope<'static>) -> Result<Variant, ScriptError> {
        let mut inner = rhai::Engine::new_raw();
        register_sandbox_base(&mut inner, &self.config);

        let started_at = Instant::now();
        let deadline = started_at + Duration::from_millis(self.config.wall_time_ms);
        inner.on_progress(move |_ops| {
            if Instant::now() >= deadline {
                Some(Dynamic::UNIT)
            } else {
                None
            }
        });

        let value = inner
            .eval_with_scope::<Dynamic>(&mut scope, source)
            .map_err(|e| map_eval_error(&self.config, *e, started_at))?;

        if let Ok(n) = value.as_int() {
            Ok(Variant::Int(n))
        } else if let Ok(f) = value.as_float() {
            Variant::float(f).map_err(|_| ScriptError::Runtime {
                reason: "non-finite result".to_owned(),
            })
        } else {
            Err(ScriptError::Runtime {
                reason: format!("expected numeric result, got {}", value.type_name()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(expr: &str) -> Result<Variant, ScriptError> {
        MathEvaluator::with_config(EngineConfig::default()).eval(expr)
    }

    #[test]
    fn eval_computes_arithmetic_with_correct_precedence_and_types() {
        for (expr, expected) in [
            ("2 + 3 * 4", Variant::Int(14)),
            ("(2 + 3) * 4", Variant::Int(20)),
            ("-5 + 2", Variant::Int(-3)),
            ("10 / 4", Variant::Int(2)),
            ("7 % 3", Variant::Int(1)),
            ("10.0 / 4.0", Variant::Float(2.5)),
            ("2.5 + 2.5", Variant::Float(5.0)),
        ] {
            assert_eq!(eval(expr).unwrap(), expected, "expr: {expr}");
        }
    }

    #[test]
    fn eval_rejects_non_finite_results() {
        for expr in ["0.0 / 0.0", "1.0 / 0.0"] {
            let err = eval(expr).unwrap_err();
            assert!(err.to_string().contains("non-finite"), "expr {expr}: {err}");
        }
    }

    #[test]
    fn eval_rejects_non_numeric_result() {
        let err = eval("\"hello\"").unwrap_err();
        assert!(
            err.to_string().contains("expected numeric result"),
            "got: {err}"
        );
    }

    #[test]
    fn eval_treats_percent_tokens_literally_without_re_interpolating() {
        assert!(eval("%kills%").is_err());
    }

    #[test]
    fn eval_past_the_op_limit_returns_the_typed_limit_error() {
        const OP_LIMIT: u64 = 500;
        let evaluator = MathEvaluator::with_config(EngineConfig {
            op_limit: OP_LIMIT,
            wall_time_ms: 30_000,
        });
        let err = evaluator
            .eval("let x = 0; while x < 100000000 { x += 1; } x")
            .unwrap_err();
        assert!(
            matches!(err, ScriptError::OperationLimit { ops: OP_LIMIT }),
            "{err:?}"
        );
    }

    #[test]
    fn eval_past_the_wall_budget_returns_the_typed_timeout() {
        let evaluator = MathEvaluator::with_config(EngineConfig {
            op_limit: 1_000_000_000,
            wall_time_ms: 1,
        });
        let err = evaluator.eval("loop {}").unwrap_err();
        assert!(matches!(err, ScriptError::Timeout { .. }), "{err:?}");
    }
}
