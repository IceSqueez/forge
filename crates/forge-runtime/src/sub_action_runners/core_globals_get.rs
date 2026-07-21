use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreGlobalsGetRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsGetRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsGetRunner {
    fn id(&self) -> &str {
        "core.globals.get"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Get Global"
    }

    fn summary(&self) -> &str {
        "Read a global variable into an argument"
    }

    fn search_text(&self) -> &str {
        "get global variable read load"
    }

    fn icon_name(&self) -> &str {
        "database-import"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("into_arg".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "name",
                label: "Variable Name",
                options_key: "global.names",
            },
            FormField::Text {
                key: "into_arg",
                label: "Output Variable",
                placeholder: "result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config
            .require_str("name")
            .and(config.require_str("into_arg"))
            .map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.get");

        let name_template = config.str("name").unwrap_or_default();
        let into_template = config.str("into_arg").unwrap_or_default();

        let resolved_name =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(name_template));
        let resolved_into =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(into_template));

        let (outcome, updated_stack) = match self.globals.get(&resolved_name).await {
            Ok(value) => {
                let variant = value.unwrap_or(Variant::String(String::new()));
                let new_stack = ctx.arg_stack.clone().set(resolved_into, variant);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        (timer.finish(outcome), updated_stack)
    }
}
