use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use rand::RngExt;
use time::OffsetDateTime;

pub struct CoreRandomIntRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreRandomIntRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreRandomIntRunner {
    fn id(&self) -> &str {
        "core.random.int"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Integer"
    }

    fn summary(&self) -> &str {
        "Generate a random integer in [min, max] and store it in a global"
    }

    fn search_text(&self) -> &str {
        "random integer number generate range"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("min".to_owned(), Variant::Int(1));
        cfg.insert("max".to_owned(), Variant::Int(100));
        cfg.insert("target_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Integer {
                key: "min",
                label: "Minimum",
                min: i64::MIN,
                max: i64::MAX,
            },
            FormField::Integer {
                key: "max",
                label: "Maximum",
                min: i64::MIN,
                max: i64::MAX,
            },
            FormField::Text {
                key: "target_var",
                label: "Target Variable",
                placeholder: "random_result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("target_var").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.random.int: target_var is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let min = config.get("min").and_then(|v| v.as_int()).unwrap_or(1);
        let max = config.get("max").and_then(|v| v.as_int()).unwrap_or(100);
        let target_var = config
            .get("target_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let outcome = if min > max {
            SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})"))
        } else {
            let value = rand::rng().random_range(min..=max);
            match self
                .globals
                .set(&target_var, Variant::Int(value), false)
                .await
            {
                Ok(()) => {
                    ctx.publisher.publish(Event::caused_by(
                        EventSource::Core,
                        "global.set",
                        serde_json::json!({
                            "key": target_var,
                            "source": "random_int",
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
                kind: "core.random.int".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
