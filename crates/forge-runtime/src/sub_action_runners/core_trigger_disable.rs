use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::TriggerInstanceRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, Variant,
};

pub struct CoreTriggerDisableRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
}

impl CoreTriggerDisableRunner {
    pub fn new(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self { trigger_instances }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config.str("trigger_instance_id").unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let Ok(instance_id) = resolved.parse::<TriggerInstanceId>() else {
            return SubActionOutcome::Failed(format!(
                "core.trigger.disable: invalid trigger_instance_id '{resolved}'"
            ));
        };
        match self.trigger_instances.set_enabled(instance_id, false).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("core.trigger.disable: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerDisableRunner {
    fn id(&self) -> &str {
        "core.trigger.disable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Disable Trigger"
    }

    fn summary(&self) -> &str {
        "Prevent a trigger instance from firing actions"
    }

    fn search_text(&self) -> &str {
        "disable trigger block deactivate stop"
    }

    fn icon_name(&self) -> &str {
        "toggle-off"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "trigger_instance_id".to_owned(),
            Variant::String(String::new()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "trigger_instance_id",
            label: "Trigger",
            options_key: "trigger_instance.ids",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("trigger_instance_id").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.trigger.disable");
        let outcome = self.run(config, ctx).await;
        (timer.finish(outcome), None)
    }
}
