use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use rand::RngExt;
use time::OffsetDateTime;

pub struct CoreRandomFloatRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreRandomFloatRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreRandomFloatRunner {
    fn id(&self) -> &str {
        "core.random.float"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Float"
    }

    fn summary(&self) -> &str {
        "Generate a random float in [min, max] and store it in a global"
    }

    fn search_text(&self) -> &str {
        "random float decimal number generate range"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("min".to_owned(), Variant::Float(0.0));
        cfg.insert("max".to_owned(), Variant::Float(1.0));
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "min",
                label: "Minimum",
                placeholder: "0.0",
            },
            FormField::Text {
                key: "max",
                label: "Maximum",
                placeholder: "1.0",
            },
            FormField::Text {
                key: "into_var",
                label: "Target Variable",
                placeholder: "random_result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("into_var").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.random.float: into_var is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let min = config.get("min").and_then(|v| v.as_float()).unwrap_or(0.0);
        let max = config.get("max").and_then(|v| v.as_float()).unwrap_or(1.0);
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let outcome = if min > max {
            SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})"))
        } else {
            let value = rand::rng().random_range(min..=max);
            match self
                .globals
                .set(&into_var, Variant::Float(value), false)
                .await
            {
                Ok(()) => {
                    ctx.publisher.publish(Event::caused_by(
                        EventSource::Core,
                        "global.set",
                        serde_json::json!({
                            "key": into_var,
                            "source": "random_float",
                            "new_value": value,
                        }),
                        ctx.parent_event_id,
                    ));
                    SubActionOutcome::Success
                }
                Err(e) => SubActionOutcome::Failed(format!("global write failed: {e}")),
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.random.float".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
