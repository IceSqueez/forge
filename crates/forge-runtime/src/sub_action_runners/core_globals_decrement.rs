use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreGlobalsDecrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDecrementRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDecrementRunner {
    fn id(&self) -> &str {
        "core.globals.decrement"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Decrement Global"
    }

    fn summary(&self) -> &str {
        "Decrement a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "decrement global variable counter subtract minus"
    }

    fn icon_name(&self) -> &str {
        "minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg.insert("amount".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "key",
                label: "Variable Name",
                placeholder: "counter",
            },
            FormField::Integer {
                key: "amount",
                label: "Amount",
                min: 0,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.decrement: key is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let key_template = config
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let amount = config.get("amount").and_then(|v| v.as_int()).unwrap_or(1);

        let resolved_key = super::interpolate::interpolate_with_globals(
            key_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match self.globals.incr(&resolved_key, -amount).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.incr",
                    serde_json::json!({
                        "key": resolved_key,
                        "delta": -amount,
                        "new_value": new_val_json,
                    }),
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
                kind: "core.globals.decrement".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
