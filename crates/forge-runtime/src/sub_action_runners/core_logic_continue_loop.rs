use async_trait::async_trait;
use forge_registry::{
    ControlSignal, FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::core_logic_shared::telemetry;

pub struct CoreLogicContinueLoopRunner;

#[async_trait]
impl SubActionRunner for CoreLogicContinueLoopRunner {
    fn id(&self) -> &str {
        "core.logic.continue_loop"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Continue Loop"
    }

    fn summary(&self) -> &str {
        "Skip to the next iteration of the innermost enclosing loop"
    }

    fn search_text(&self) -> &str {
        "continue loop skip next iteration flow control"
    }

    fn icon_name(&self) -> &str {
        "arrow-redo"
    }

    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        ctx.control.set(ControlSignal::Continue);
        (
            telemetry(ctx, self.id(), started_at, SubActionOutcome::Success),
            None,
        )
    }
}
