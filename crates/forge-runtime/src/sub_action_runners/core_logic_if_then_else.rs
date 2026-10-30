use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChainLayout, CodeField, CodeKind, CodeLanguage, ConfigProblem, ConfigSpec, RunContext,
    StepChainField, StepTimer, SubActionCategory, SubActionRunner, ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

use super::core_logic_shared::{chain_at, propagate, retag};
use crate::core_messages;
use crate::{ConditionError, ConditionGate};

const BRANCH_TAKEN_VAR: &str = "branch.taken";

const CONDITION: CodeField = CodeField {
    key: "condition",
    label: "Condition",
    language: CodeLanguage::Rhai,
    kind: CodeKind::Expression,
    default: "",
    required: true,
};

const UNDEFINED_IS_FALSE: ToggleField = ToggleField {
    key: "treat_undefined_as_false",
    label: "Treat undefined as false",
    default: true,
};

const THEN_CHAIN: StepChainField = StepChainField {
    key: "then_chain",
    label: "Then",
    layout: ChainLayout::Steps,
    seeds_empty_list: false,
};

const ELSE_CHAIN: StepChainField = StepChainField {
    key: "else_chain",
    label: "Else",
    layout: ChainLayout::Steps,
    seeds_empty_list: false,
};

const CONFIG: ConfigSpec =
    ConfigSpec::new(&[&CONDITION, &UNDEFINED_IS_FALSE, &THEN_CHAIN, &ELSE_CHAIN]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, bool), ConfigProblem> {
    Ok((
        CONDITION.read(config, args)?,
        UNDEFINED_IS_FALSE.read(config, args)?,
    ))
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (condition, undefined_is_false) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let verdict = match self
            .gate
            .evaluate_with_args(&condition, ctx.arg_stack)
            .await
        {
            Ok(value) => value,
            Err(ConditionError::Undefined { .. }) if undefined_is_false => false,
            Err(e) => {
                return (
                    timer.failed(core_messages::condition_failed(e).into()),
                    None,
                );
            }
        };

        let (branch, chain) = if verdict {
            ("then", THEN_CHAIN.key)
        } else {
            ("else", ELSE_CHAIN.key)
        };
        let steps = chain_at(config, chain, ctx);
        let base = ctx.arg_stack.clone().set(
            BRANCH_TAKEN_VAR.to_owned(),
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
                    BRANCH_TAKEN_VAR.to_owned(),
                    Variant::String(branch.to_owned()),
                );
                (timer.finish(outcome), Some(stack))
            }
            Err(e) => (
                timer.failed(core_messages::condition_failed(e).into()),
                None,
            ),
        }
    }
}
