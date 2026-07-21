use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreGlobalsDeleteRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDeleteRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDeleteRunner {
    fn id(&self) -> &str {
        "core.globals.delete"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Delete Global"
    }

    fn summary(&self) -> &str {
        "Remove a global variable"
    }

    fn search_text(&self) -> &str {
        "delete global variable remove clear"
    }

    fn icon_name(&self) -> &str {
        "database-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "name",
            label: "Variable Name",
            options_key: "global.names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.delete");

        let name_template = config.str("name").unwrap_or_default();

        let resolved_name =
            forge_types::strip_var_decoration(&ctx.arg_stack.interpolate(name_template));

        let outcome = match self.globals.delete(&resolved_name).await {
            Ok(_existed) => {
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.del",
                    serde_json::json!({ "key": resolved_name }),
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}
