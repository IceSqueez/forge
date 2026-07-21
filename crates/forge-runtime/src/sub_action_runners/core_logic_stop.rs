use async_trait::async_trait;
use forge_registry::{
    ControlSignal, FormField, RegistryError, RunContext, StepTimer, StopMark, SubActionCategory,
    SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant};

pub struct CoreLogicStopRunner;

#[async_trait]
impl SubActionRunner for CoreLogicStopRunner {
    fn id(&self) -> &str {
        "core.logic.stop"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Stop Action"
    }

    fn summary(&self) -> &str {
        "Halt the current action chain immediately"
    }

    fn search_text(&self) -> &str {
        "stop halt end action chain abort flow control"
    }

    fn icon_name(&self) -> &str {
        "stop-circle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "mark_as".to_owned(),
            Variant::String("completed".to_owned()),
        );
        cfg.insert("reason".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Select {
                key: "mark_as",
                label: "Mark As",
                options: &["completed", "failed"],
            },
            FormField::Text {
                key: "reason",
                label: "Reason",
                placeholder: "why the chain stopped",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let failed = config
            .str("mark_as")
            .is_some_and(|s| s.eq_ignore_ascii_case("failed"));

        let reason = config
            .str("reason")
            .map(|raw| ctx.arg_stack.interpolate(raw))
            .filter(|s| !s.is_empty());

        ctx.control
            .set(ControlSignal::Stop(StopMark { failed, reason }));

        (timer.success(), None)
    }
}
