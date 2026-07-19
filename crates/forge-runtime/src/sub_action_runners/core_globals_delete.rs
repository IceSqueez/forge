use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        match config.get("name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.delete: name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let name_template = config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let resolved_name = super::interpolate::interpolate_with_globals(
            name_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.delete".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
