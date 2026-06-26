use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreRandomBoolRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreRandomBoolRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreRandomBoolRunner {
    fn id(&self) -> &str {
        "core.random.bool"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Bool (Weighted)"
    }

    fn summary(&self) -> &str {
        "Generate a weighted random boolean and store it in a global"
    }

    fn search_text(&self) -> &str {
        "random bool boolean weighted probability chance true false"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("probability_true".to_owned(), Variant::Float(0.5));
        cfg.insert("into_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "probability_true",
                label: "Probability of True (0..1)",
                placeholder: "0.5",
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
                "core.random.bool: into_var is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let probability = config
            .get("probability_true")
            .and_then(|v| v.as_float())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let value = rand::random_bool(probability);

        let outcome = match self
            .globals
            .set(&into_var, Variant::Bool(value), false)
            .await
        {
            Ok(()) => {
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.set",
                    serde_json::json!({
                        "key": into_var,
                        "source": "random_bool",
                        "new_value": value,
                    }),
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(format!("global write failed: {e}")),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.random.bool".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
