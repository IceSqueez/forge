use std::time::{Duration, Instant};

use forge_types::Variant;
use rhai::Dynamic;

use crate::ScriptError;
use crate::engine::{EngineConfig, map_eval_error, register_sandbox_base};

/// Carries no `ForgeApi`; the globals/chat/http surface is structurally unreachable rather than merely unused.
#[derive(Debug, Clone)]
pub struct MathEvaluator {
    config: EngineConfig,
}

impl MathEvaluator {
    pub fn with_config(config: EngineConfig) -> Self {
        Self { config }
    }

    /// NaN and infinite results are rejected rather than surviving into a `Variant`.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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
    fn eval_bounded_expression_terminates_with_error_quickly() {
        let evaluator = MathEvaluator::with_config(EngineConfig {
            op_limit: 500,
            wall_time_ms: 50,
        });
        let start = Instant::now();
        let result = evaluator.eval("let x = 0; while x < 100000000 { x += 1; } x");
        let elapsed = start.elapsed();
        assert!(result.is_err(), "unbounded expression should be rejected");
        assert!(
            elapsed < Duration::from_secs(5),
            "evaluation must not hang: took {elapsed:?}"
        );
    }
}
