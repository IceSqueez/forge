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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
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

    struct FailingGlobals;
    #[async_trait]
    impl GlobalsRepo for FailingGlobals {
        async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(None)
        }
        async fn set(&self, _n: &str, _v: Variant, _p: bool) -> Result<(), StorageError> {
            Err(StorageError::Connection {
                reason: "backend down".to_owned(),
            })
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

    fn cfg(min: f64, max: f64, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("min".to_owned(), Variant::Float(min));
        c.insert("max".to_owned(), Variant::Float(max));
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    async fn run(runner: &CoreRandomFloatRunner, cfg: &SubActionConfig) -> SubActionOutcome {
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
    async fn float_result_always_lands_within_inclusive_bounds() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        let cfg = cfg(2.0, 5.0, "r");
        for _ in 0..200 {
            assert!(matches!(
                run(&runner, &cfg).await,
                SubActionOutcome::Success
            ));
            match globals.last().unwrap().1 {
                Variant::Float(f) => assert!((2.0..=5.0).contains(&f), "out of bounds: {f}"),
                other => panic!("expected Float, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn float_min_equals_max_yields_that_exact_value() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        run(&runner, &cfg(3.5, 3.5, "r")).await;
        assert_eq!(globals.last().unwrap().1, Variant::Float(3.5));
    }

    #[tokio::test]
    async fn float_min_greater_than_max_fails_and_writes_nothing() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        let outcome = run(&runner, &cfg(5.0, 1.0, "r")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(globals.count(), 0);
    }

    #[tokio::test]
    async fn float_writes_non_persisted_float_global_under_target_var() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        run(&runner, &cfg(0.0, 1.0, "my_var")).await;
        let (key, value, persisted) = globals.last().unwrap();
        assert_eq!(key, "my_var");
        assert!(!persisted, "random output must not be persisted");
        assert!(matches!(value, Variant::Float(_)));
    }

    #[tokio::test]
    async fn float_global_write_failure_reports_failed_outcome() {
        let runner = CoreRandomFloatRunner::new(Arc::new(FailingGlobals));
        let outcome = run(&runner, &cfg(0.0, 1.0, "r")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }
}
