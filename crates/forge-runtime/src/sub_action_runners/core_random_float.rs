use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, SubActionCategory, SubActionIo,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use rand::RngExt;
use time::OffsetDateTime;

pub struct CoreRandomFloatRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreRandomFloatRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }

    async fn resolve_bound(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
        key: &str,
        default: f64,
    ) -> Result<f64, String> {
        let raw = match config.get(key) {
            Some(Variant::Float(n)) => return Ok(*n),
            Some(Variant::Int(n)) => return Ok(*n as f64),
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
            .parse::<f64>()
            .map_err(|_| format!("{key} is not a valid number: {resolved:?}"))
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
        "Generate a random float in [min, max] and store it in a variable"
    }

    fn search_text(&self) -> &str {
        "random float decimal number generate range"
    }

    fn icon_name(&self) -> &str {
        "dice"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("min".to_owned(), Variant::String("0.0".to_owned()));
        cfg.insert("max".to_owned(), Variant::String("1.0".to_owned()));
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

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Float,
                label: "Random float".to_owned(),
            }],
            consumes: Vec::new(),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let min = self.resolve_bound(config, ctx, "min", 0.0).await;
        let max = self.resolve_bound(config, ctx, "max", 1.0).await;
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let (outcome, produced) = match (min, max) {
            (Err(e), _) | (Ok(_), Err(e)) => (SubActionOutcome::Failed(e), None),
            (Ok(min), Ok(max)) if min > max => (
                SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})")),
                None,
            ),
            (Ok(min), Ok(max)) => {
                let value = rand::rng().random_range(min..=max);
                let stack = ctx.arg_stack.clone().set(into_var, Variant::Float(value));
                (SubActionOutcome::Success, Some(stack))
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
            produced,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;
    use std::sync::Mutex;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    // The runner keeps a GlobalsRepo handle for INPUT interpolation only; it
    // must never write its output back to globals. This mock records every
    // `set` so tests can assert the output path leaves `count()` at zero -
    // a live regression guard against the old write-to-globals behavior.
    #[derive(Default)]
    struct RecordingGlobals {
        writes: Mutex<Vec<(String, Variant, bool)>>,
    }

    impl RecordingGlobals {
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

    fn cfg_v(min: Variant, max: Variant, into: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("min".to_owned(), min);
        c.insert("max".to_owned(), max);
        c.insert("into_var".to_owned(), Variant::String(into.to_owned()));
        c
    }

    fn cfg(min: f64, max: f64, into: &str) -> SubActionConfig {
        cfg_v(Variant::Float(min), Variant::Float(max), into)
    }

    async fn run(
        runner: &CoreRandomFloatRunner,
        cfg: &SubActionConfig,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(cfg, &ctx).await;
        (telemetry.outcome, produced)
    }

    #[tokio::test]
    async fn float_result_always_lands_within_inclusive_bounds() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        let cfg = cfg(2.0, 5.0, "r");
        for _ in 0..200 {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert!(matches!(outcome, SubActionOutcome::Success));
            match produced.unwrap().get("r") {
                Some(Variant::Float(f)) => {
                    assert!((2.0..=5.0).contains(f), "out of bounds: {f}")
                }
                other => panic!("expected Float in scope, got {other:?}"),
            }
        }
        assert_eq!(globals.count(), 0, "output must not be written to globals");
    }

    #[tokio::test]
    async fn float_min_equals_max_yields_that_exact_value() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        let (_outcome, produced) = run(&runner, &cfg(3.5, 3.5, "r")).await;
        assert_eq!(produced.unwrap().get("r"), Some(&Variant::Float(3.5)));
    }

    #[tokio::test]
    async fn float_failure_paths_produce_no_stack_and_write_nothing() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreRandomFloatRunner::new(globals.clone());
        let cases = [
            ("min greater than max", cfg(5.0, 1.0, "r")),
            (
                "unparseable min",
                cfg_v(Variant::String("abc".to_owned()), Variant::Float(1.0), "r"),
            ),
        ];
        for (label, cfg) in cases {
            let (outcome, produced) = run(&runner, &cfg).await;
            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "{label} must fail"
            );
            assert!(produced.is_none(), "{label} must not produce a scope stack");
        }
        assert_eq!(globals.count(), 0, "failures must not write globals");
    }
}
