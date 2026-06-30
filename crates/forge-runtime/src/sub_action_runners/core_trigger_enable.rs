use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::TriggerInstanceRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, Variant,
};
use time::OffsetDateTime;

pub struct CoreTriggerEnableRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
}

impl CoreTriggerEnableRunner {
    pub fn new(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self { trigger_instances }
    }

    async fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config
            .get("trigger_instance_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let Ok(instance_id) = resolved.parse::<TriggerInstanceId>() else {
            return SubActionOutcome::Failed(format!(
                "core.trigger.enable: invalid trigger_instance_id '{resolved}'"
            ));
        };
        match self.trigger_instances.set_enabled(instance_id, true).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(format!("core.trigger.enable: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerEnableRunner {
    fn id(&self) -> &str {
        "core.trigger.enable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Enable Trigger"
    }

    fn summary(&self) -> &str {
        "Allow a trigger instance to fire actions"
    }

    fn search_text(&self) -> &str {
        "enable trigger allow activate"
    }

    fn icon_name(&self) -> &str {
        "toggle-on"
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
        match config.get("trigger_instance_id").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.trigger.enable: trigger_instance_id is required".to_owned(),
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
                index: ctx.index,
                kind: "core.trigger.enable".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
