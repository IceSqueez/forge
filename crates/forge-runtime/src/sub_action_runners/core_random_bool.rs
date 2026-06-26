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

    fn cfg(probability_true: f64, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert(
            "probability_true".to_owned(),
            Variant::Float(probability_true),
        );
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    async fn run(runner: &CoreRandomBoolRunner, cfg: &SubActionConfig) -> SubActionOutcome {
        let stack = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        };
        runner.execute(cfg, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn bool_probability_edges_are_deterministic_and_clamped() {
        // 1.0 / 0.0 are the in-range deterministic boundaries; 2.0 / -1.0 are
        // out of range and would panic in `random_bool` without the clamp.
        for (probability, expected) in [(1.0, true), (0.0, false), (2.0, true), (-1.0, false)] {
            let globals = Arc::new(RecordingGlobals::default());
            let runner = CoreRandomBoolRunner::new(globals.clone());
            let cfg = cfg(probability, "r");
            for _ in 0..50 {
                assert!(matches!(
                    run(&runner, &cfg).await,
                    SubActionOutcome::Success
                ));
                assert_eq!(
                    globals.last().unwrap().1,
                    Variant::Bool(expected),
                    "probability={probability}"
                );
            }
        }
    }

    #[tokio::test]
    async fn bool_writes_non_persisted_bool_global_under_target_var() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomBoolRunner::new(globals.clone());
        run(&runner, &cfg(1.0, "flag")).await;
        let (key, value, persisted) = globals.last().unwrap();
        assert_eq!(key, "flag");
        assert!(!persisted, "random output must not be persisted");
        assert!(matches!(value, Variant::Bool(_)));
    }
}
