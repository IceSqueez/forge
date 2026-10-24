use std::sync::Arc;

use super::json_object_input::{EMPTY_JSON_OBJECT, read_json_object, validate_json_object};
use crate::{QueueAdmission, SchedulerCell, SchedulerRequest};
use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, Variant,
};

const OUTPUTS_KEY: &str = "override_outputs";

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

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw_id = config.str("trigger_instance_id").unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw_id);
        let Ok(instance_id) = resolved.parse::<TriggerInstanceId>() else {
            return SubActionOutcome::Failed(
                "core.test.fire_trigger: invalid trigger_instance_id".to_owned(),
            );
        };

        let trigger_kind = match self.trigger_instances.get(instance_id).await {
            Ok(Some(instance)) => instance.kind_id,
            Ok(None) => {
                return SubActionOutcome::Failed(format!(
                    "core.test.fire_trigger: unknown trigger instance '{instance_id}'"
                ));
            }
            Err(e) => {
                return SubActionOutcome::Failed(format!(
                    "core.test.fire_trigger: instance lookup failed: {e}"
                ));
            }
        };

        let Some(scheduler) = self.scheduler.get() else {
            return SubActionOutcome::Failed("queue scheduler not ready".to_owned());
        };

        let action_ids = match self.trigger_instances.actions_using(instance_id).await {
            Ok(ids) => ids,
            Err(e) => {
                return SubActionOutcome::Failed(format!(
                    "core.test.fire_trigger: bound-action lookup failed: {e}"
                ));
            }
        };

        let synthetic_args = match synthetic_outputs(config) {
            Ok(args) => args,
            Err(reason) => {
                return SubActionOutcome::Failed(format!("core.test.fire_trigger: {reason}"));
            }
        };

        for action_id in action_ids {
            let action = match self.actions.get(action_id).await {
                Ok(Some(a)) => a,
                Ok(None) => continue,
                Err(e) => {
                    return SubActionOutcome::Failed(format!(
                        "core.test.fire_trigger: action lookup failed: {e}"
                    ));
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
                return SubActionOutcome::Failed(format!(
                    "core.test.fire_trigger: dispatch failed: {e}"
                ));
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "trigger_instance_id".to_owned(),
            Variant::String(String::new()),
        );
        cfg.insert(
            OUTPUTS_KEY.to_owned(),
            Variant::String(EMPTY_JSON_OBJECT.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "trigger_instance_id",
                label: "Trigger Instance",
                options_key: "trigger_instance.ids",
            },
            FormField::TextArea {
                key: OUTPUTS_KEY,
                label: "Synthetic Outputs (JSON object)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("trigger_instance_id")?;
        validate_json_object(config, OUTPUTS_KEY)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.test.fire_trigger");
        let outcome = self.run(config, ctx).await;
        (timer.finish(outcome), None)
    }
}

fn synthetic_outputs(config: &SubActionConfig) -> Result<ArgStack, String> {
    Ok(read_json_object(config, OUTPUTS_KEY)?
        .into_iter()
        .fold(ArgStack::new(), |stack, (key, value)| stack.set(key, value)))
}
