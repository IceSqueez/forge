use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::references::TRIGGER_INSTANCE_OPTIONS_KEY;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DynamicSelectField, JsonObjectField, RunContext,
    StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, Variant,
};

use super::core_trigger_shared::TRIGGER_INSTANCE_ID_KEY;
use super::event_payload::EMPTY_JSON_OBJECT;
use crate::core_messages;
use crate::{QueueAdmission, SchedulerCell, SchedulerRequest};

const TRIGGER_INSTANCE: DynamicSelectField<TriggerInstanceId> = DynamicSelectField::new(
    TRIGGER_INSTANCE_ID_KEY,
    "Trigger Instance",
    TRIGGER_INSTANCE_OPTIONS_KEY,
);

const OUTPUTS: JsonObjectField = JsonObjectField {
    key: "override_outputs",
    label: "Synthetic Outputs (JSON object)",
    default: EMPTY_JSON_OBJECT,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TRIGGER_INSTANCE, &OUTPUTS]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(TriggerInstanceId, ArgStack), ConfigProblem> {
    Ok((
        TRIGGER_INSTANCE.read(config, args)?,
        synthetic_outputs(OUTPUTS.read(config, args)?)?,
    ))
}

fn synthetic_outputs(
    outputs: serde_json::Map<String, serde_json::Value>,
) -> Result<ArgStack, ConfigProblem> {
    outputs
        .into_iter()
        .try_fold(ArgStack::new(), |stack, (key, value)| {
            Variant::from_plain_json(value)
                .map(|variant| stack.set(key, variant))
                .map_err(|error| {
                    ConfigProblem::field(
                        OUTPUTS.key,
                        ConfigReason::Coded(core_messages::output_not_convertible(error)),
                    )
                })
        })
}

pub struct CoreTestFireTriggerRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    actions: Arc<dyn ActionRepo>,
    scheduler: SchedulerCell,
}

impl CoreTestFireTriggerRunner {
    pub fn new(
        trigger_instances: Arc<dyn TriggerInstanceRepo>,
        actions: Arc<dyn ActionRepo>,
        scheduler: SchedulerCell,
    ) -> Self {
        Self {
            trigger_instances,
            actions,
            scheduler,
        }
    }

    async fn run(
        &self,
        instance_id: TriggerInstanceId,
        synthetic_args: ArgStack,
        ctx: &RunContext<'_>,
    ) -> SubActionOutcome {
        let trigger_kind = match self.trigger_instances.get(instance_id).await {
            Ok(Some(instance)) => instance.kind_id,
            Ok(None) => {
                return SubActionOutcome::Failed(
                    core_messages::unknown_trigger_instance(instance_id).into(),
                );
            }
            Err(e) => {
                return SubActionOutcome::Failed(core_messages::storage_failed(e).into());
            }
        };

        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed(core_messages::scheduler_not_ready().into());
        };

        let action_ids = match self.trigger_instances.actions_using(instance_id).await {
            Ok(ids) => ids,
            Err(e) => {
                return SubActionOutcome::Failed(core_messages::storage_failed(e).into());
            }
        };

        for action_id in action_ids {
            let action = match self.actions.get(action_id).await {
                Ok(Some(a)) => a,
                Ok(None) => continue,
                Err(e) => {
                    return SubActionOutcome::Failed(core_messages::storage_failed(e).into());
                }
            };
            let req = SchedulerRequest {
                queue_id: action.queue_id,
                action_id,
                trigger_event_id: ctx.parent_event_id,
                trigger_kind: Some(trigger_kind.clone()),
                manual_test: false,
                initial_args: synthetic_args.clone(),
                admission: QueueAdmission::from(&action),
            };
            if let Err(e) = scheduler.dispatch(req).await {
                return SubActionOutcome::Failed(core_messages::scheduler_failed(e).into());
            }
        }

        SubActionOutcome::Success
    }
}

#[async_trait]
impl SubActionRunner for CoreTestFireTriggerRunner {
    fn id(&self) -> &str {
        "core.test.fire_trigger"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Test Trigger"
    }

    fn summary(&self) -> &str {
        "Fire one trigger instance with synthetic outputs to run its actions"
    }

    fn search_text(&self) -> &str {
        "test trigger fire manual simulate synthetic outputs"
    }

    fn icon_name(&self) -> &str {
        "bolt"
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
        let (instance_id, synthetic_args) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(instance_id, synthetic_args, ctx).await;
        (timer.finish(outcome), None)
    }
}
