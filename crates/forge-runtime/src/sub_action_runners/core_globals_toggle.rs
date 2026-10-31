use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{EventSource, GlobalToggled, TypedEvent};
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_storage::{GlobalsRepo, StorageError};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_globals_shared::{GLOBAL_BY_KEY, global_name};
use crate::core_messages;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_KEY]);

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let resolved_key = match global_name(&GLOBAL_BY_KEY, config, ctx.arg_stack) {
            Ok(key) => key,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = match self.globals.toggle(&resolved_key).await {
            Ok(flipped) => {
                GlobalToggled {
                    key: resolved_key.clone(),
                    new_value: flipped,
                    prev_value: !flipped,
                }
                .publish(
                    ctx.publisher,
                    EventSource::Core,
                    Some(ctx.parent_event_id),
                );
                SubActionOutcome::Success
            }
            Err(StorageError::NotFound { .. }) => {
                SubActionOutcome::Failed(core_messages::global_missing(&resolved_key).into())
            }
            Err(StorageError::TypeMismatch { actual, .. }) => {
                SubActionOutcome::Failed(core_messages::global_not_bool(&actual).into())
            }
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::failure::failure_message;
    use forge_events::Event;
    use forge_events::EventPublisher;
    use forge_events::test_support::NullPublisher;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;
    use forge_types::Variant;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);
    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[derive(Default)]
    struct MapGlobals {
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
        let reason = failure_message(&outcome);
        assert_eq!(reason.code(), &core_messages::GLOBAL_NOT_BOOL);
        assert_eq!(
            reason.arg("actual"),
            Some(&forge_types::MessageArg::ValueKind(
                forge_types::VariantKind::Int
            ))
        );
        assert_eq!(
            globals.snapshot("flag"),
            Some(Variant::Int(7)),
            "value must be untouched on failure"
        );
    }

    #[tokio::test]
    async fn toggle_runner_emits_global_toggled_with_both_bools() {
        for (start, flipped) in [(true, false), (false, true)] {
            let globals = Arc::new(MapGlobals::with([("flag", Variant::Bool(start))]));
            let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
            let publisher = CapturingPublisher(Arc::clone(&events));
            let parent = EventId::new();
            let stack = ArgStack::new();
            let ctx = RunContext::leaf(&stack, 0, parent, &publisher);
            let runner = CoreGlobalsToggleRunner::new(globals);
            let outcome = runner.execute(&cfg("flag"), &ctx).await.0.outcome;

            assert!(matches!(outcome, SubActionOutcome::Success));
            let captured = events.lock().unwrap();
            let ev = &captured[0];
            assert_eq!(ev.kind, "global.toggled");
            assert_eq!(ev.caused_by, Some(parent));
            assert_eq!(ev.payload["key"], "flag");
            assert_eq!(ev.payload["new_value"].as_bool(), Some(flipped));
            assert_eq!(ev.payload["prev_value"].as_bool(), Some(start));
        }
    }
}
