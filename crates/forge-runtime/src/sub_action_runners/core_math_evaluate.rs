use crate::core_messages;
use async_trait::async_trait;
use forge_registry::{
    ChoiceField, CodeField, CodeKind, CodeLanguage, ConfigChoice, ConfigProblem, ConfigSpec,
    ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner,
    TypedField, VariableField,
};
use forge_script::{BoundExpression, EngineConfig, MathEvaluator};
use forge_types::{
    ArgStack, MessageRef, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
    VariantKind,
};

const INTO_VAR_KEY: &str = "into_var";
const RESULT_VARIABLE: &str = "result";
const EXPRESSION_OP_LIMIT: u64 = 50_000;
const EXPRESSION_WALL_TIME_MS: u64 = 100;
const I64_RANGE: std::ops::Range<f64> = -(i64::MAX as f64)..(i64::MAX as f64);

#[derive(Clone, Copy)]
enum ResultType {
    Auto,
    Int,
    Float,
}

impl ResultType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Int => "int",
            Self::Float => "float",
        }
    }

    fn coerce(self, value: Variant) -> Result<Variant, MessageRef> {
        match (self, value) {
            (Self::Int, Variant::Float(f)) if I64_RANGE.contains(&f) => Ok(Variant::Int(f as i64)),
            (Self::Int, Variant::Float(f)) => Err(core_messages::output_not_convertible(f)),
            (Self::Float, Variant::Int(n)) => Ok(Variant::Float(n as f64)),
            (_, other) => Ok(other),
        }
    }
}

impl ConfigChoice for ResultType {
    const ALL: &'static [Self] = &[Self::Auto, Self::Int, Self::Float];
    const WIRE_NAMES: &'static [&'static str] = &[
        Self::Auto.as_str(),
        Self::Int.as_str(),
        Self::Float.as_str(),
    ];

    fn wire_name(self) -> &'static str {
        self.as_str()
    }
}

const EXPRESSION: CodeField = CodeField {
    key: "expression",
    label: "Expression",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Expression,
    default: "",
    required: true,
};

const INTO_VAR: VariableField = VariableField {
    key: INTO_VAR_KEY,
    label: "Output Variable",
    placeholder: RESULT_VARIABLE,
    default: RESULT_VARIABLE,
};

const RESULT_TYPE: ChoiceField<ResultType> = ChoiceField {
    key: "result_type",
    label: "Result Type",
    default: ResultType::Auto,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&EXPRESSION, &INTO_VAR, &RESULT_TYPE]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, ResultType), ConfigProblem> {
    Ok((
        EXPRESSION.read(config, args)?,
        INTO_VAR.read(config, args)?,
        RESULT_TYPE.read(config, args)?,
    ))
}

pub struct CoreMathEvaluateRunner {
    evaluator: MathEvaluator,
}

impl Default for CoreMathEvaluateRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreMathEvaluateRunner {
    pub fn new() -> Self {
        Self {
            evaluator: MathEvaluator::with_config(EngineConfig {
                op_limit: EXPRESSION_OP_LIMIT,
                wall_time_ms: EXPRESSION_WALL_TIME_MS,
            }),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreMathEvaluateRunner {
    fn id(&self) -> &str {
        "core.math.evaluate"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Math Expression"
    }

    fn summary(&self) -> &str {
        "Evaluate an arithmetic expression and store the result as an argument"
    }

    fn search_text(&self) -> &str {
        "math expression calculate arithmetic formula evaluate number compute"
    }

    fn icon_name(&self) -> &str {
        "calculator"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::Float,
                label: "Result".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (expression, into_var, result_type) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let bound = BoundExpression::bind(&expression, ctx.arg_stack);
        let evaluator = self.evaluator.clone();
        let evaluated = tokio::task::spawn_blocking(move || {
            let result = evaluator.eval_bound(&bound);
            match result {
                Err(_) if !bound.unresolved().is_empty() => Err(
                    core_messages::expression_undefined_variables(bound.unresolved()),
                ),
                other => other.map_err(core_messages::expression_failed),
            }
        })
        .await
        .unwrap_or_else(|e| Err(core_messages::task_failed(e)));

        let (outcome, updated_stack) = match evaluated.and_then(|v| result_type.coerce(v)) {
            Ok(variant) => {
                let new_stack = ctx.arg_stack.clone().set(into_var, variant);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(reason) => (SubActionOutcome::Failed(reason.into()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;

    fn cfg(expr: &str, into: &str, result_type: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("expression".to_owned(), Variant::String(expr.to_owned()));
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c.insert(
            "result_type".to_owned(),
            Variant::String(result_type.to_owned()),
        );
        c
    }

    async fn run(cfg: &SubActionConfig) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, out) = CoreMathEvaluateRunner::new().execute(cfg, &ctx).await;
        (telemetry.outcome, out)
    }

    #[tokio::test]
    async fn evaluate_stores_numeric_result_under_named_var() {
        let (outcome, out) = run(&cfg("2 + 3", "total", "auto")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(out.unwrap().get("total"), Some(&Variant::Int(5)));
    }

    #[tokio::test]
    async fn evaluate_result_type_int_truncates_float_value() {
        let (_, out) = run(&cfg("10.0 / 4.0", "x", "int")).await;
        assert_eq!(out.unwrap().get("x"), Some(&Variant::Int(2)));
    }

    #[tokio::test]
    async fn evaluate_result_type_float_promotes_int_value() {
        let (_, out) = run(&cfg("5", "x", "float")).await;
        assert_eq!(out.unwrap().get("x"), Some(&Variant::Float(5.0)));
    }

    #[tokio::test]
    async fn evaluate_with_a_blank_output_var_fails_instead_of_writing_a_default_name() {
        let (outcome, out) = run(&cfg("1 + 1", "", "auto")).await;
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(m) if crate::test_support::failure::is_field_problem(m, "into_var", "config.required")),
            "{outcome:?}"
        );
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn evaluate_failed_expression_yields_failed_and_no_stack() {
        let (outcome, out) = run(&cfg("\"not a number\"", "x", "auto")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }

    const TWO_TO_THE_63: f64 = 9_223_372_036_854_775_808.0;
    const NEXT_FLOAT_BELOW_TWO_TO_THE_63: f64 = 9_223_372_036_854_774_784.0;
    const NEXT_FLOAT_BELOW_MINUS_TWO_TO_THE_63: f64 = -9_223_372_036_854_777_856.0;

    #[test]
    fn int_coercion_keeps_every_float_that_fits_i64() {
        for (float, expected) in [
            (-TWO_TO_THE_63, i64::MIN),
            (NEXT_FLOAT_BELOW_TWO_TO_THE_63, 9_223_372_036_854_774_784),
            (-0.5, 0),
        ] {
            assert_eq!(
                ResultType::Int.coerce(Variant::Float(float)).unwrap(),
                Variant::Int(expected),
                "{float}"
            );
        }
    }

    #[test]
    fn int_coercion_refuses_a_float_that_does_not_fit_i64() {
        for float in [
            TWO_TO_THE_63,
            NEXT_FLOAT_BELOW_MINUS_TWO_TO_THE_63,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let refusal = ResultType::Int.coerce(Variant::Float(float)).unwrap_err();
            assert_eq!(
                refusal.code(),
                &core_messages::OUTPUT_NOT_CONVERTIBLE,
                "{float}"
            );
        }
    }

    #[tokio::test]
    async fn evaluate_result_type_int_fails_the_step_when_the_value_does_not_fit() {
        let (outcome, out) = run(&cfg("1e300", "x", "int")).await;

        assert!(
            matches!(outcome, SubActionOutcome::Failed(_)),
            "{outcome:?}"
        );
        assert!(out.is_none());
    }
}
