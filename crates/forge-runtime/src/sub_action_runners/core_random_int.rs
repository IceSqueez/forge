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

    async fn resolve_bound(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
        key: &str,
        default: i64,
    ) -> Result<i64, String> {
        let raw = match config.get(key) {
            Some(Variant::Int(n)) => return Ok(*n),
            Some(Variant::String(s)) => s.clone(),
            _ => return Ok(default),
        };
        if raw.trim().is_empty() {
            return Ok(default);
        }
        let resolved = super::interpolate::interpolate_with_globals(
            &raw,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        resolved
            .trim()
            .parse::<i64>()
            .map_err(|_| format!("{key} is not a valid integer: {resolved:?}"))
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
        cfg.insert("min".to_owned(), Variant::String("1".to_owned()));
        cfg.insert("max".to_owned(), Variant::String("100".to_owned()));
        cfg.insert("target_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "min",
                label: "Minimum",
                placeholder: "1",
            },
            FormField::Text {
                key: "max",
                label: "Maximum",
                placeholder: "100",
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

        let min = self.resolve_bound(config, ctx, "min", 1).await;
        let max = self.resolve_bound(config, ctx, "max", 100).await;
        let target_var = config
            .get("target_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let outcome = match (min, max) {
            (Err(e), _) | (Ok(_), Err(e)) => SubActionOutcome::Failed(e),
            (Ok(min), Ok(max)) if min > max => {
                SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})"))
            }
            (Ok(min), Ok(max)) => {
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
