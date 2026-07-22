use async_trait::async_trait;
use forge_registry::{
    CodeLanguage, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_script::{EngineConfig, MathEvaluator};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreMathEvaluateRunner {
    evaluator: MathEvaluator,
}

impl Default for CoreMathEvaluateRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreMathEvaluateRunner {
    /// Tighter than a full script's budget, but generous enough for complex arithmetic.
    pub fn new() -> Self {
        Self {
            evaluator: MathEvaluator::with_config(EngineConfig {
                op_limit: 50_000,
                wall_time_ms: 100,
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("expression".to_owned(), Variant::String(String::new()));
        cfg.insert("into_var".to_owned(), Variant::String("result".to_owned()));
        cfg.insert("result_type".to_owned(), Variant::String("auto".to_owned()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Code {
                key: "expression",
                label: "Expression",
                language: CodeLanguage::Rhai,
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "result",
            },
            FormField::Select {
                key: "result_type",
                label: "Result Type",
                options: &["auto", "int", "float"],
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("expression").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.math.evaluate");

        let expression = config.str("expression").unwrap_or_default();

        let into_var =
            forge_types::strip_var_decoration(config.str_nonempty("into_var").unwrap_or("result"));

        let result_type = config.str("result_type").unwrap_or("auto");

        let (outcome, updated_stack) = match self.evaluator.eval(expression) {
            Ok(variant) => {
                let coerced = match result_type {
                    "int" => match variant {
                        Variant::Float(f) => Variant::Int(f as i64),
                        other => other,
                    },
                    "float" => match variant {
                        Variant::Int(n) => Variant::Float(n as f64),
                        other => other,
                    },
                    _ => variant,
                };
                let new_stack = ctx.arg_stack.clone().set(into_var, coerced);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

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
    async fn evaluate_defaults_output_var_to_result_when_blank() {
        let (outcome, out) = run(&cfg("1 + 1", "", "auto")).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(out.unwrap().get("result"), Some(&Variant::Int(2)));
    }

    #[tokio::test]
    async fn evaluate_failed_expression_yields_failed_and_no_stack() {
        let (outcome, out) = run(&cfg("\"not a number\"", "x", "auto")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(out.is_none());
    }
}
