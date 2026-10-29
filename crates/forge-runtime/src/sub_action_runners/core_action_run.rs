use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ChainSignal, ConfigSpec, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, ToggleField, TypedField,
};
use forge_storage::ActionRepo;
use forge_types::{
    Action, ActionId, ArgStack, ExecutionMode, SubActionConfig, SubActionOutcome, SubActionStep,
    SubActionTelemetry,
};
use rand::RngExt;

use super::core_action_shared::ACTION;

const INHERIT_ARGS: ToggleField = ToggleField {
    key: "inherit_args",
    label: "Pass current arguments",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ACTION, &INHERIT_ARGS]);

pub struct CoreActionRunRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionRunRunner {
    pub fn new(actions: Arc<dyn ActionRepo>) -> Self {
        Self { actions }
    }

    async fn run(
        &self,
        action_id: ActionId,
        inherit_args: bool,
        ctx: &RunContext<'_>,
    ) -> SubActionOutcome {
        let target = match self.actions.get(action_id).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                return SubActionOutcome::Failed(format!(
                    "core.action.run: unknown action '{action_id}'"
                ));
            }
            Err(e) => return SubActionOutcome::Failed(format!("core.action.run: {e}")),
        };

        let child_stack = if inherit_args {
            ctx.arg_stack.clone()
        } else {
            ArgStack::new()
        };

        let steps = pick_steps(&target);
        let outcome = ctx
            .executor
            .run_child_chain(&steps, &child_stack, ctx.parent_event_id)
            .await;

        match outcome {
            Err(RegistryError::DepthExceeded(_)) => {
                SubActionOutcome::Failed("core.action.run: max nesting depth exceeded".to_owned())
            }
            Err(e) => SubActionOutcome::Failed(format!("core.action.run: {e}")),
            Ok(child) => match child.signal {
                ChainSignal::Completed
                | ChainSignal::Stop(_)
                | ChainSignal::Break
                | ChainSignal::Continue => SubActionOutcome::Success,
                ChainSignal::Error(msg) => SubActionOutcome::Failed(msg),
                ChainSignal::Aborted => {
                    SubActionOutcome::Failed("core.action.run: cancelled".to_owned())
                }
            },
        }
    }
}

fn pick_steps(target: &Action) -> Vec<SubActionStep> {
    if target.execution_mode == ExecutionMode::RandomPick && !target.sub_actions.is_empty() {
        let idx = rand::rng().random_range(0..target.sub_actions.len());
        return vec![target.sub_actions[idx].clone()];
    }
    target.sub_actions.clone()
}

#[async_trait]
impl SubActionRunner for CoreActionRunRunner {
    fn id(&self) -> &str {
        "core.action.run"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Run Action"
    }

    fn summary(&self) -> &str {
        "Run another action's steps from inside this chain"
    }

    fn search_text(&self) -> &str {
        "run action call execute chain nested"
    }

    fn icon_name(&self) -> &str {
        "play"
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
        let (action_id, inherit_args) = match ACTION
            .read(config, ctx.arg_stack)
            .and_then(|action_id| Ok((action_id, INHERIT_ARGS.read(config, ctx.arg_stack)?)))
        {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(action_id, inherit_args, ctx).await;
        (timer.finish(outcome), None)
    }
}
