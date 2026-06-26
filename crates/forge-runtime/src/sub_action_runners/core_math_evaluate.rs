use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_script::{EngineConfig, MathEvaluator};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreMathEvaluateRunner {
    evaluator: MathEvaluator,
}

impl Default for CoreMathEvaluateRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreMathEvaluateRunner {
    /// Op and wall budgets are tighter than a full script but generous enough
    /// for complex arithmetic — the risk of looping is low but must still be bounded.
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
            FormField::Text {
                key: "expression",
                label: "Expression",
                placeholder: "(%kills% * 100) / %deaths%",
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
        let expr_ok = config
            .get("expression")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if expr_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.math.evaluate: expression is required".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let expression = config
            .get("expression")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("result")
            .to_owned();

        let result_type = config
            .get("result_type")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.math.evaluate".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
        )
    }
}
