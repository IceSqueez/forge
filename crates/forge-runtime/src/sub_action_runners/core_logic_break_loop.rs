use async_trait::async_trait;
use forge_registry::{
    ControlSignal, FormField, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

pub struct CoreLogicBreakLoopRunner;

#[async_trait]
impl SubActionRunner for CoreLogicBreakLoopRunner {
    fn id(&self) -> &str {
        "core.logic.break_loop"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Break Loop"
    }

    fn summary(&self) -> &str {
        "Exit the innermost enclosing loop"
    }

    fn search_text(&self) -> &str {
        "break loop exit stop iteration flow control"
    }

    fn icon_name(&self) -> &str {
        "arrow-out"
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
        let timer = StepTimer::start(ctx, self.id());
        ctx.control.set(ControlSignal::Break);
        (timer.success(), None)
    }
}
