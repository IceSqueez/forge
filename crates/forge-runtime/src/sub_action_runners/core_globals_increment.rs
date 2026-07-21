use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreGlobalsIncrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsIncrementRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsIncrementRunner {
    fn id(&self) -> &str {
        "core.globals.increment"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Increment Global"
    }

    fn summary(&self) -> &str {
        "Increment a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "increment global variable counter add"
    }

    fn icon_name(&self) -> &str {
        "plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("amount".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "name",
                label: "Variable Name",
                options_key: "global.names",
            },
            FormField::Integer {
                key: "amount",
                label: "Amount",
                min: i64::MIN,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.increment");

        let name_template = config.str("name").unwrap_or_default();
        let amount = config.int("amount").unwrap_or(1);

        let resolved_name =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(name_template));

        let outcome = match self.globals.incr(&resolved_name, amount).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.incr",
                    serde_json::json!({
                        "key": resolved_name,
                        "delta": amount,
                        "new_value": new_val_json,
                    }),
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}
