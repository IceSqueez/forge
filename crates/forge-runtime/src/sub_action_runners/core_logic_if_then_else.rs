use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    CodeLanguage, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

use super::core_logic_shared::{decode_chain, propagate, retag};
use crate::ConditionGate;

pub struct CoreLogicIfThenElseRunner {
    gate: Arc<ConditionGate>,
}

impl CoreLogicIfThenElseRunner {
    pub fn new(gate: Arc<ConditionGate>) -> Self {
        Self { gate }
    }
}

#[async_trait]
impl SubActionRunner for CoreLogicIfThenElseRunner {
    fn id(&self) -> &str {
        "core.logic.if_then_else"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "If / Then / Else"
    }

    fn summary(&self) -> &str {
        "Run one of two sub-chains depending on a condition"
    }

    fn search_text(&self) -> &str {
        "if then else condition branch conditional flow control"
    }

    fn icon_name(&self) -> &str {
        "git-branch"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("condition".to_owned(), Variant::String(String::new()));
        cfg.insert("treat_undefined_as_false".to_owned(), Variant::Bool(true));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Code {
                key: "condition",
                label: "Condition",
                language: CodeLanguage::Rhai,
            },
            FormField::Toggle {
                key: "treat_undefined_as_false",
                label: "Treat undefined as false",
            },
            FormField::SubChain {
                key: "then_chain",
                label: "Then",
            },
            FormField::SubChain {
                key: "else_chain",
                label: "Else",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("condition").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let template = config.str("condition").unwrap_or_default();
        let expr = ctx.arg_stack.interpolate(template);
        let undefined_is_false = config.bool("treat_undefined_as_false").unwrap_or(true);

        let verdict = match self.gate.evaluate(&expr).await {
            Ok(value) => value,
            Err(_) if undefined_is_false => false,
            Err(e) => {
                return (timer.failed(format!("core.logic.if_then_else: {e}")), None);
            }
        };

        let branch = if verdict { "then" } else { "else" };
        let steps = decode_chain(config, if verdict { "then_chain" } else { "else_chain" });
        let base = ctx.arg_stack.clone().set(
            "branch.taken".to_owned(),
            Variant::String(branch.to_owned()),
        );

        match ctx
            .executor
            .run_child_chain(&steps, &base, ctx.parent_event_id)
            .await
        {
            Ok(child) => {
                ctx.telemetry
                    .extend(retag(child.telemetry, ctx.index, branch));
                let outcome = propagate(child.signal, ctx);
                let stack = child.arg_stack.set(
                    "branch.taken".to_owned(),
                    Variant::String(branch.to_owned()),
                );
                (timer.finish(outcome), Some(stack))
            }
            Err(e) => (timer.failed(format!("core.logic.if_then_else: {e}")), None),
        }
    }
}
