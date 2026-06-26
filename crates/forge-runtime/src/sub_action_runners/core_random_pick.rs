use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use rand::RngExt;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use time::OffsetDateTime;

pub struct CoreRandomPickRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreRandomPickRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreRandomPickRunner {
    fn id(&self) -> &str {
        "core.random.pick"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Random Pick"
    }

    fn summary(&self) -> &str {
        "Pick a random element from a list and store it in a global"
    }

    fn search_text(&self) -> &str {
        "random pick choose select list element weighted"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("items".to_owned(), Variant::Array(vec![]));
        cfg.insert("weights".to_owned(), Variant::Array(vec![]));
        cfg.insert("into_var".to_owned(), Variant::String("picked".to_owned()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "items",
                label: "Items (one per line)",
            },
            FormField::TextArea {
                key: "weights",
                label: "Weights (one per line, empty = uniform)",
            },
            FormField::Text {
                key: "into_var",
                label: "Target Variable",
                placeholder: "picked",
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
        let started_at = OffsetDateTime::now_utc();

        let items: Vec<Variant> = match config.get("items") {
            Some(Variant::Array(arr)) => arr.clone(),
            Some(v) => v
                .as_str()
                .unwrap_or("")
                .lines()
                .filter(|s| !s.is_empty())
                .map(|s| Variant::String(s.to_owned()))
                .collect(),
            None => vec![],
        };

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("picked")
            .to_owned();

        let outcome = if items.is_empty() {
            SubActionOutcome::Failed("items list must not be empty".to_owned())
        } else {
            let weights: Vec<f64> = match config.get("weights") {
                Some(Variant::Array(arr)) => arr.iter().filter_map(|v| v.as_float()).collect(),
                Some(v) => v
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect(),
                None => vec![],
            };

            let idx = if weights.is_empty() {
                Ok(rand::rng().random_range(0..items.len()))
            } else if weights.len() != items.len() {
                Err(format!(
                    "weights length ({}) must match items length ({})",
                    weights.len(),
                    items.len()
                ))
            } else {
                WeightedIndex::new(&weights)
                    .map(|dist| dist.sample(&mut rand::rng()))
                    .map_err(|e| format!("invalid weights: {e}"))
            };

            match idx {
                Err(msg) => SubActionOutcome::Failed(msg),
                Ok(i) => {
                    let picked = items[i].clone();
                    match self.globals.set(&into_var, picked.clone(), false).await {
                        Ok(()) => {
                            ctx.publisher.publish(Event::caused_by(
                                EventSource::Core,
                                "global.set",
                                serde_json::json!({
                                    "key": into_var,
                                    "source": "random_pick",
                                    "new_value": picked.to_string(),
                                }),
                                ctx.parent_event_id,
                            ));
                            SubActionOutcome::Success
                        }
                        Err(e) => SubActionOutcome::Failed(format!("global write failed: {e}")),
                    }
                }
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.random.pick".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
