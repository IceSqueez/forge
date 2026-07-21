use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

pub struct CoreGlobalsToggleRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsToggleRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsToggleRunner {
    fn id(&self) -> &str {
        "core.globals.toggle"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Toggle Global"
    }

    fn summary(&self) -> &str {
        "Flip a boolean global variable between true and false"
    }

    fn search_text(&self) -> &str {
        "toggle global variable bool boolean flip switch"
    }

    fn icon_name(&self) -> &str {
        "toggle"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "key",
            label: "Variable Name",
            options_key: "global.names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("key").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.toggle");

        let key_template = config.str("key").unwrap_or_default();

        let resolved_key =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(key_template));

        let outcome = match self.globals.get(&resolved_key).await {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.globals.toggle: global '{}' does not exist",
                resolved_key
            )),
            Ok(Some(Variant::Bool(b))) => {
                let flipped = Variant::Bool(!b);
                let persisted = self
                    .globals
                    .persisted(&resolved_key)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false);
                match self.globals.set(&resolved_key, flipped, persisted).await {
                    Ok(()) => {
                        ctx.publisher.publish(Event::caused_by(
                            EventSource::Core,
                            "global.set",
                            serde_json::json!({
                                "key": resolved_key,
                                "new_value": !b,
                            }),
                            ctx.parent_event_id,
                        ));
                        SubActionOutcome::Success
                    }
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                }
            }
            Ok(Some(other)) => SubActionOutcome::Failed(format!(
                "core.globals.toggle: expected bool, found {}",
                VariantKind::from_variant(&other).label()
            )),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::EventPublisher;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[derive(Default)]
    struct MapGlobals {
        // value + its stored `persisted` flag, so the runner's read-then-preserve
        // path can be observed end to end.
        map: Mutex<BTreeMap<String, (Variant, bool)>>,
    }

    impl MapGlobals {
        fn with(entries: impl IntoIterator<Item = (&'static str, Variant)>) -> Self {
            Self::seeded(entries.into_iter().map(|(k, v)| (k, v, false)))
        }
        fn seeded(entries: impl IntoIterator<Item = (&'static str, Variant, bool)>) -> Self {
            let map = entries
                .into_iter()
                .map(|(k, v, p)| (k.to_owned(), (v, p)))
                .collect();
            Self {
                map: Mutex::new(map),
            }
        }
        fn snapshot(&self, key: &str) -> Option<Variant> {
            self.map.lock().unwrap().get(key).map(|(v, _)| v.clone())
        }
        fn persisted_flag(&self, key: &str) -> Option<bool> {
            self.map.lock().unwrap().get(key).map(|(_, p)| *p)
        }
    }

    #[async_trait]
    impl GlobalsRepo for MapGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).map(|(v, _)| v.clone()))
        }
        async fn set(&self, name: &str, value: Variant, p: bool) -> Result<(), StorageError> {
            self.map.lock().unwrap().insert(name.to_owned(), (value, p));
            Ok(())
        }
        async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).map(|(_, p)| *p))
        }
        async fn delete(&self, name: &str) -> Result<bool, StorageError> {
            Ok(self.map.lock().unwrap().remove(name).is_some())
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

    fn cfg(key: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("key".to_owned(), Variant::String(key.to_owned()));
        c
    }

    async fn run(globals: Arc<MapGlobals>, config: &SubActionConfig) -> SubActionOutcome {
        let runner = CoreGlobalsToggleRunner::new(globals);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn toggle_flips_bool_in_both_directions() {
        for (start, flipped) in [(true, false), (false, true)] {
            let globals = Arc::new(MapGlobals::with([("flag", Variant::Bool(start))]));
            let outcome = run(globals.clone(), &cfg("flag")).await;
            assert!(matches!(outcome, SubActionOutcome::Success));
            assert_eq!(
                globals.snapshot("flag"),
                Some(Variant::Bool(flipped)),
                "{start} should flip to {flipped}"
            );
        }
    }

    #[tokio::test]
    async fn toggle_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals, &cfg("ghost")).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn toggle_preserves_the_stored_persisted_flag() {
        // Regression for CORE-2 / DT-05-F12: the flip must reuse the global's
        // existing persistence, not demote it to session by hardcoding false.
        for flag in [true, false] {
            let globals = Arc::new(MapGlobals::seeded([("flag", Variant::Bool(true), flag)]));
            let outcome = run(globals.clone(), &cfg("flag")).await;
            assert!(matches!(outcome, SubActionOutcome::Success));
            assert_eq!(globals.snapshot("flag"), Some(Variant::Bool(false)));
            assert_eq!(
                globals.persisted_flag("flag"),
                Some(flag),
                "persisted flag must survive the toggle (was {flag})"
            );
        }
    }

    #[tokio::test]
    async fn toggle_non_bool_global_reports_failed_naming_the_type() {
        let globals = Arc::new(MapGlobals::with([("flag", Variant::Int(7))]));
        let outcome = run(globals.clone(), &cfg("flag")).await;
        match outcome {
            SubActionOutcome::Failed(msg) => assert!(
                msg.contains("INT"),
                "type-mismatch message should name the found type, got: {msg}"
            ),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(
            globals.snapshot("flag"),
            Some(Variant::Int(7)),
            "value must be untouched on failure"
        );
    }
}
