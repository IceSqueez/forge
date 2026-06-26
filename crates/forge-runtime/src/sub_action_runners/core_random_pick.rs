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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventPublisher;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;
    use std::sync::Mutex;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[derive(Default)]
    struct RecordingGlobals {
        writes: Mutex<Vec<(String, Variant, bool)>>,
    }

    impl RecordingGlobals {
        fn last(&self) -> Option<(String, Variant, bool)> {
            self.writes.lock().unwrap().last().cloned()
        }
        fn count(&self) -> usize {
            self.writes.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl GlobalsRepo for RecordingGlobals {
        async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(None)
        }
        async fn set(
            &self,
            name: &str,
            value: Variant,
            persisted: bool,
        ) -> Result<(), StorageError> {
            self.writes
                .lock()
                .unwrap()
                .push((name.to_owned(), value, persisted));
            Ok(())
        }
        async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
            Ok(vec![])
        }
        async fn storage_bytes(&self) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
            Ok(Variant::Int(0))
        }
    }

    fn str_array(items: &[&str]) -> Variant {
        Variant::Array(
            items
                .iter()
                .map(|s| Variant::String((*s).to_owned()))
                .collect(),
        )
    }

    fn cfg(items: Variant, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("items".to_owned(), items);
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    async fn run(runner: &CoreRandomPickRunner, cfg: &SubActionConfig) -> SubActionOutcome {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(cfg, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn pick_result_is_always_an_element_of_the_list() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        let cfg = cfg(str_array(&["a", "b", "c"]), "r");
        for _ in 0..200 {
            assert!(matches!(
                run(&runner, &cfg).await,
                SubActionOutcome::Success
            ));
            let picked = globals.last().unwrap().1;
            assert!(
                matches!(&picked, Variant::String(s) if ["a", "b", "c"].contains(&s.as_str())),
                "picked outside list: {picked:?}"
            );
        }
    }

    #[tokio::test]
    async fn pick_single_element_list_stores_that_element_unpersisted() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        run(&runner, &cfg(str_array(&["only"]), "r")).await;
        let (key, value, persisted) = globals.last().unwrap();
        assert_eq!(key, "r");
        assert_eq!(value, Variant::String("only".to_owned()));
        assert!(!persisted, "random output must not be persisted");
    }

    #[tokio::test]
    async fn pick_empty_list_fails_without_panic() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        let outcome = run(&runner, &cfg(Variant::Array(vec![]), "r")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(globals.count(), 0);
    }

    #[tokio::test]
    async fn pick_never_selects_a_zero_weight_element() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        let mut cfg = cfg(str_array(&["never", "always"]), "r");
        cfg.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Float(0.0), Variant::Float(1.0)]),
        );
        for _ in 0..100 {
            assert!(matches!(
                run(&runner, &cfg).await,
                SubActionOutcome::Success
            ));
            assert_eq!(
                globals.last().unwrap().1,
                Variant::String("always".to_owned())
            );
        }
    }

    #[tokio::test]
    async fn pick_weights_length_mismatch_fails() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        let mut cfg = cfg(str_array(&["a", "b"]), "r");
        cfg.insert(
            "weights".to_owned(),
            Variant::Array(vec![Variant::Float(1.0)]),
        );
        let outcome = run(&runner, &cfg).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(globals.count(), 0);
    }

    #[tokio::test]
    async fn pick_parses_newline_delimited_string_items() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        let cfg = cfg(Variant::String("x\ny\nz".to_owned()), "r");
        for _ in 0..100 {
            assert!(matches!(
                run(&runner, &cfg).await,
                SubActionOutcome::Success
            ));
            let picked = globals.last().unwrap().1;
            assert!(
                matches!(&picked, Variant::String(s) if ["x", "y", "z"].contains(&s.as_str())),
                "picked outside parsed list: {picked:?}"
            );
        }
    }

    #[tokio::test]
    async fn pick_defaults_target_var_to_picked_when_blank() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomPickRunner::new(globals.clone());
        run(&runner, &cfg(str_array(&["only"]), "")).await;
        assert_eq!(globals.last().unwrap().0, "picked");
    }
}
