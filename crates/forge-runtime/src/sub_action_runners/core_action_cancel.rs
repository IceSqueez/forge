use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

use crate::action_cancel::ActionCancelRegistry;

pub struct CoreActionCancelRunner {
    cancel_registry: Arc<ActionCancelRegistry>,
}

impl CoreActionCancelRunner {
    pub fn new(cancel_registry: Arc<ActionCancelRegistry>) -> Self {
        Self { cancel_registry }
    }

    fn run(&self, config: &SubActionConfig, ctx: &RunContext<'_>) -> SubActionOutcome {
        let raw = config
            .get("action_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(raw);
        let resolved = resolved.trim();

        if resolved.is_empty() {
            self.cancel_registry.cancel_all();
            return SubActionOutcome::Success;
        }

        let Ok(action_id) = resolved.parse::<ActionId>() else {
            return SubActionOutcome::Failed(format!(
                "core.action.cancel: invalid action_id '{resolved}'"
            ));
        };
        self.cancel_registry.cancel(action_id);
        SubActionOutcome::Success
    }
}

#[async_trait]
impl SubActionRunner for CoreActionCancelRunner {
    fn id(&self) -> &str {
        "core.action.cancel"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Cancel Action"
    }

    fn summary(&self) -> &str {
        "Stop in-flight runs of an action (or all running actions)"
    }

    fn search_text(&self) -> &str {
        "cancel action stop abort kill running"
    }

    fn icon_name(&self) -> &str {
        "octagon-x"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("action_id".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "action_id",
            label: "Action",
            options_key: "action.ids",
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let outcome = self.run(config, ctx);
        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;
        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.action.cancel".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
