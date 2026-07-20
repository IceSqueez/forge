use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::{ActionRepo, TriggerInstanceRepo};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, Variant,
};
use std::collections::BTreeMap;
use time::OffsetDateTime;

use crate::{SchedulerCell, SchedulerRequest};

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
        let raw_id = config
            .get("trigger_instance_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw_id);
        let Ok(instance_id) = resolved.parse::<TriggerInstanceId>() else {
            return SubActionOutcome::Failed(format!(
                "core.test.fire_trigger: invalid trigger_instance_id '{resolved}'"
            ));
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

        let synthetic_args = synthetic_outputs(config);

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
                initial_args: synthetic_args.clone(),
                bypass_pause: action.bypass_pause,
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
            "override_outputs".to_owned(),
            Variant::Object(BTreeMap::new()),
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
                key: "override_outputs",
                label: "Synthetic Outputs (JSON object)",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("trigger_instance_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.test.fire_trigger: trigger_instance_id is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let outcome = self.run(config, ctx).await;
        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.test.fire_trigger".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn synthetic_outputs(config: &SubActionConfig) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(Variant::Object(map)) = config.get("override_outputs") {
        for (key, value) in map {
            stack = stack.set(key.clone(), value.clone());
        }
    }
    stack
}
