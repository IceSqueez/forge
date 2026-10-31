use crate::error::ScriptError;

use rhai::Dynamic;

use crate::arg_binding::BoundExpression;
use crate::engine::{EngineConfig, budgeted_sandbox, map_eval_error};
use crate::script_thread::on_script_stack;
use crate::wall_budget::SandboxClock;

#[derive(Debug, Clone)]
pub struct ConditionEvaluator {
    config: EngineConfig,
    clock: SandboxClock,
}

impl ConditionEvaluator {
    pub fn with_config(config: EngineConfig) -> Self {
        Self::on_clock(config, SandboxClock::default())
    }

    pub(crate) fn on_clock(config: EngineConfig, clock: SandboxClock) -> Self {
        Self { config, clock }
    }

    pub fn eval(&self, expr: &str) -> Result<bool, ScriptError> {
        self.run(expr, expr, rhai::Scope::new())
    }

    pub fn eval_bound(&self, expr: &BoundExpression) -> Result<bool, ScriptError> {
        self.run(expr.original(), expr.source(), expr.scope())
    }

    fn run(
        &self,
        label: &str,
        source: &str,
        mut scope: rhai::Scope<'static>,
    ) -> Result<bool, ScriptError> {
        on_script_stack(move || {
            let (inner, budget) = budgeted_sandbox(&self.config, self.clock.clone());

            let value = inner
                .eval_with_scope::<Dynamic>(&mut scope, source)
                .map_err(|e| map_eval_error(&self.config, *e, budget.elapsed()))?;

            value
                .as_bool()
                .map_err(|got| ScriptError::ConditionNotBoolean {
                    expr: label.to_owned(),
                    got: got.to_owned(),
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScriptedClock;
    use std::time::Duration;

    const WALL_LIMIT_MS: u64 = 1_000;
    const WALL_LIMIT: Duration = Duration::from_millis(WALL_LIMIT_MS);

    fn bounded() -> EngineConfig {
        EngineConfig {
            op_limit: 10_000,
            wall_time_ms: WALL_LIMIT_MS,
        }
    }

    fn eval(expr: &str) -> Result<bool, ScriptError> {
        ConditionEvaluator::with_config(EngineConfig::default()).eval(expr)
    }

    #[test]
    fn eval_returns_the_boolean_each_comparison_operator_computes() {
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
        let evaluator = ConditionEvaluator::with_config(EngineConfig {
            op_limit: 100,
            wall_time_ms: 30_000,
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
    fn eval_exceeding_wall_budget_returns_typed_timeout() {
        let clock = ScriptedClock::ticking(WALL_LIMIT);
        let evaluator = ConditionEvaluator::on_clock(bounded(), clock.sandbox());
        let err = evaluator.eval("loop {}").unwrap_err();
        assert!(
            matches!(
                err,
                ScriptError::Timeout {
                    limit_ms: WALL_LIMIT_MS,
                    ..
                }
            ),
            "expected Timeout, got {err:?}",
        );
    }

    #[test]
    fn a_stopped_clock_leaves_only_the_op_limit_to_end_a_runaway_condition() {
        let evaluator = ConditionEvaluator::on_clock(bounded(), ScriptedClock::frozen().sandbox());
        let err = evaluator.eval("loop {}").unwrap_err();
        assert!(matches!(err, ScriptError::OperationLimit { .. }), "{err:?}");
    }
}
