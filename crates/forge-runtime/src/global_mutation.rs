use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{
    EventPublisher, EventSource, GlobalDeleted, GlobalIncremented, GlobalSet, TypedEvent,
};
use forge_script::{GlobalDeletion, GlobalIncrement, ScriptGlobals, ScriptGlobalsError};
use forge_storage::{GlobalsRepo, StorageError};
use forge_types::{EventId, Variant};

pub(crate) struct GlobalMutation<'a> {
    pub globals: &'a dyn GlobalsRepo,
    pub publisher: &'a dyn EventPublisher,
    pub source: EventSource,
    pub caused_by: EventId,
}

impl GlobalMutation<'_> {
    pub async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
    ) -> Result<Option<Variant>, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        let new_value_json = value.to_plain_json();
        let prev_value_json = previous
            .as_ref()
            .map(Variant::to_plain_json)
            .unwrap_or(serde_json::Value::Null);
        self.globals.set(key, value, persisted).await?;
        self.publish(GlobalSet {
            key: key.to_owned(),
            new_value: new_value_json,
            prev_value: prev_value_json,
            persisted: None,
            via: None,
        });
        Ok(previous)
    }

    pub async fn increment(&self, key: &str, delta: i64) -> Result<GlobalIncrement, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        let current = self.globals.incr(key, delta).await?;
        let new_value_json = match &current {
            Variant::Int(i) => serde_json::Value::from(*i),
            _ => serde_json::Value::String(current.to_string()),
        };
        self.publish(GlobalIncremented {
            key: key.to_owned(),
            delta,
            new_value: new_value_json,
        });
        Ok(GlobalIncrement { previous, current })
    }

    pub async fn delete(&self, key: &str) -> Result<GlobalDeletion, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        if !self.globals.delete(key).await? {
            return Ok(GlobalDeletion::Missing);
        }
        self.publish(GlobalDeleted {
            key: key.to_owned(),
        });
        Ok(GlobalDeletion::Deleted { previous })
    }

    fn publish(&self, payload: impl TypedEvent) {
        payload.publish(self.publisher, self.source, Some(self.caused_by));
    }
}

pub struct RuntimeScriptGlobals {
    globals: Arc<dyn GlobalsRepo>,
    publisher: Arc<dyn EventPublisher>,
}

impl RuntimeScriptGlobals {
    pub fn new(globals: Arc<dyn GlobalsRepo>, publisher: Arc<dyn EventPublisher>) -> Self {
        Self { globals, publisher }
    }

    fn mutation(&self, caused_by: EventId) -> GlobalMutation<'_> {
        GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: self.publisher.as_ref(),
            source: EventSource::Rhai,
            caused_by,
        }
    }
}

fn port_error(error: StorageError) -> ScriptGlobalsError {
    ScriptGlobalsError(error.to_string())
}

#[async_trait]
impl ScriptGlobals for RuntimeScriptGlobals {
    async fn get(&self, key: &str) -> Result<Option<Variant>, ScriptGlobalsError> {
        self.globals.get(key).await.map_err(port_error)
    }

    async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
        caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError> {
        self.mutation(caused_by)
            .set(key, value, persisted)
            .await
            .map_err(port_error)
    }

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError> {
        self.mutation(caused_by)
            .increment(key, delta)
            .await
            .map_err(port_error)
    }

    async fn delete(
        &self,
        key: &str,
        caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError> {
        self.mutation(caused_by)
            .delete(key)
            .await
            .map_err(port_error)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use forge_registry::{RunContext, SubActionRunner};
    use forge_script::{Engine, EngineConfig, ForgeApi, ScriptError};
    use forge_storage::GlobalEntry;
    use forge_types::{ArgStack, SubActionConfig, SubActionOutcome};
    use time::OffsetDateTime;

    use super::*;
    use crate::sub_action_runners::{
        CoreGlobalsDeleteRunner, CoreGlobalsIncrementRunner, CoreGlobalsSetRunner,
    };
    use forge_events::Event;

    const UNREADABLE: &str = "unreadable";

    #[derive(Default)]
    struct MapGlobals {
        map: Mutex<BTreeMap<String, Variant>>,
    }

    impl MapGlobals {
        fn seeded(entries: &[(&str, Variant)]) -> Arc<Self> {
            let map = entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            Arc::new(Self {
                map: Mutex::new(map),
            })
        }

        fn stored(&self, key: &str) -> Option<Variant> {
            self.map.lock().unwrap().get(key).cloned()
        }
    }

    #[async_trait]
    impl GlobalsRepo for MapGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            if name == UNREADABLE {
                return Err(StorageError::Decryption);
            }
            Ok(self.stored(name))
        }

        async fn set(&self, name: &str, value: Variant, _: bool) -> Result<(), StorageError> {
            self.map.lock().unwrap().insert(name.to_owned(), value);
            Ok(())
        }

        async fn delete(&self, name: &str) -> Result<bool, StorageError> {
            Ok(self.map.lock().unwrap().remove(name).is_some())
        }

        async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
            Ok(Vec::new())
        }

        async fn storage_bytes(&self) -> Result<u64, StorageError> {
            Ok(0)
        }

        async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
            let mut map = self.map.lock().unwrap();
            match map.get(name) {
                Some(Variant::Int(current)) => {
                    let next = Variant::Int(current + amount);
                    map.insert(name.to_owned(), next.clone());
                    Ok(next)
                }
                _ => Err(StorageError::NotFound {
                    key: name.to_owned(),
                }),
            }
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl Recorder {
        fn events(&self) -> Vec<Event> {
            self.0.lock().unwrap().clone()
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Mutation {
        Set {
            key: &'static str,
            raw: &'static str,
            literal: &'static str,
        },
        Increment {
            key: &'static str,
            delta: i64,
        },
        Delete {
            key: &'static str,
        },
    }

    impl Mutation {
        fn script(self) -> String {
            match self {
                Mutation::Set { key, literal, .. } => {
                    format!(r#"forge::globals::set("{key}", {literal}, false)"#)
                }
                Mutation::Increment { key, delta } => {
                    format!(r#"forge::globals::incr("{key}", {delta})"#)
                }
                Mutation::Delete { key } => format!(r#"forge::globals::del("{key}")"#),
            }
        }

        async fn run_as_sub_action(
            self,
            globals: Arc<dyn GlobalsRepo>,
            publisher: &Recorder,
            cause: EventId,
        ) -> SubActionOutcome {
            let mut config = SubActionConfig::new();
            let runner: Box<dyn SubActionRunner> = match self {
                Mutation::Set { key, raw, .. } => {
                    config.insert("name".to_owned(), Variant::String(key.to_owned()));
                    config.insert("value".to_owned(), Variant::String(raw.to_owned()));
                    Box::new(CoreGlobalsSetRunner::new(globals))
                }
                Mutation::Increment { key, delta } => {
                    config.insert("name".to_owned(), Variant::String(key.to_owned()));
                    config.insert("amount".to_owned(), Variant::Int(delta));
                    Box::new(CoreGlobalsIncrementRunner::new(globals))
                }
                Mutation::Delete { key } => {
                    config.insert("name".to_owned(), Variant::String(key.to_owned()));
                    Box::new(CoreGlobalsDeleteRunner::new(globals))
                }
            };
            let stack = ArgStack::new();
            let ctx = RunContext::leaf(&stack, 0, cause, publisher);
            runner.execute(&config, &ctx).await.0.outcome
        }

        async fn run_as_script(
            self,
            globals: Arc<dyn GlobalsRepo>,
            publisher: Arc<Recorder>,
            cause: EventId,
        ) -> Result<Option<bool>, ScriptError> {
            let port = RuntimeScriptGlobals::new(
                globals,
                Arc::clone(&publisher) as Arc<dyn EventPublisher>,
            );
            let api = ForgeApi::new(
                publisher,
                Arc::new(port),
                cause,
                Instant::now() + Duration::from_secs(30),
            );
            let engine = Engine::with_api(
                EngineConfig {
                    wall_time_ms: 30_000,
                    ..EngineConfig::default()
                },
                api,
            );
            let script = self.script();
            tokio::task::spawn_blocking(move || {
                engine
                    .eval_script(&script)
                    .map(|value| value.as_bool().ok())
            })
            .await
            .unwrap()
        }
    }

    fn prior_state() -> Vec<(&'static str, Variant)> {
        vec![
            (
                "score",
                Variant::Array(vec![Variant::Int(1), Variant::String("a".to_owned())]),
            ),
            ("hits", Variant::Int(10)),
            ("temp", Variant::Float(2.5)),
        ]
    }

    fn mutations() -> [Mutation; 5] {
        [
            Mutation::Set {
                key: "score",
                raw: "42",
                literal: "42",
            },
            Mutation::Set {
                key: "fresh",
                raw: "hello",
                literal: r#""hello""#,
            },
            Mutation::Increment {
                key: "hits",
                delta: 3,
            },
            Mutation::Increment {
                key: "hits",
                delta: -12,
            },
            Mutation::Delete { key: "temp" },
        ]
    }

    async fn events_from_both_paths(
        mutation: Mutation,
        cause: EventId,
    ) -> (Vec<Event>, Vec<Event>) {
        let script_publisher = Arc::new(Recorder::default());
        let script = mutation
            .run_as_script(
                MapGlobals::seeded(&prior_state()),
                Arc::clone(&script_publisher),
                cause,
            )
            .await;
        assert!(script.is_ok(), "{mutation:?}: {script:?}");

        let runner_publisher = Recorder::default();
        let outcome = mutation
            .run_as_sub_action(MapGlobals::seeded(&prior_state()), &runner_publisher, cause)
            .await;
        assert!(
            matches!(outcome, SubActionOutcome::Success),
            "{mutation:?}: {outcome:?}"
        );

        (script_publisher.events(), runner_publisher.events())
    }

    #[tokio::test]
    async fn a_script_and_a_sub_action_publish_the_same_kind_and_payload() {
        for mutation in mutations() {
            let (script, runner) = events_from_both_paths(mutation, EventId::new()).await;

            let shape = |events: &[Event]| -> Vec<(String, serde_json::Value)> {
                events
                    .iter()
                    .map(|event| (event.kind.clone(), event.payload.clone()))
                    .collect()
            };
            assert_eq!(shape(&script).len(), 1, "{mutation:?}");
            assert_eq!(shape(&script), shape(&runner), "{mutation:?}");
        }
    }

    #[tokio::test]
    async fn a_script_mutation_is_sourced_rhai_and_a_sub_action_mutation_core() {
        for mutation in mutations() {
            let (script, runner) = events_from_both_paths(mutation, EventId::new()).await;

            let sources = |events: &[Event]| events.iter().map(|e| e.source).collect::<Vec<_>>();
            assert_eq!(sources(&script), [EventSource::Rhai], "{mutation:?}");
            assert_eq!(sources(&runner), [EventSource::Core], "{mutation:?}");
        }
    }

    #[tokio::test]
    async fn both_paths_publish_the_triggering_event_as_cause() {
        for mutation in mutations() {
            let cause = EventId::new();
            let (script, runner) = events_from_both_paths(mutation, cause).await;

            for event in script.iter().chain(&runner) {
                assert_eq!(event.caused_by, Some(cause), "{mutation:?}");
            }
        }
    }

    #[tokio::test]
    async fn a_script_deleting_a_missing_key_returns_false_and_publishes_nothing() {
        let publisher = Arc::new(Recorder::default());

        let deleted = Mutation::Delete { key: "ghost" }
            .run_as_script(
                MapGlobals::seeded(&prior_state()),
                Arc::clone(&publisher),
                EventId::new(),
            )
            .await
            .unwrap();

        assert_eq!(deleted, Some(false));
        assert!(publisher.events().is_empty(), "{:?}", publisher.events());
    }

    #[tokio::test]
    async fn a_sub_action_deleting_a_missing_key_succeeds_and_publishes_nothing() {
        let publisher = Recorder::default();

        let outcome = Mutation::Delete { key: "ghost" }
            .run_as_sub_action(
                MapGlobals::seeded(&prior_state()),
                &publisher,
                EventId::new(),
            )
            .await;

        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        assert!(publisher.events().is_empty(), "{:?}", publisher.events());
    }

    #[tokio::test]
    async fn a_failed_script_increment_raises_and_publishes_nothing() {
        let publisher = Arc::new(Recorder::default());

        let result = Mutation::Increment {
            key: "ghost",
            delta: 1,
        }
        .run_as_script(
            MapGlobals::seeded(&prior_state()),
            Arc::clone(&publisher),
            EventId::new(),
        )
        .await;

        assert!(
            matches!(result, Err(ScriptError::Runtime { .. })),
            "{result:?}"
        );
        assert!(publisher.events().is_empty(), "{:?}", publisher.events());
    }

    fn port_over(globals: Arc<MapGlobals>) -> RuntimeScriptGlobals {
        RuntimeScriptGlobals::new(globals, Arc::new(Recorder::default()))
    }

    #[tokio::test]
    async fn each_mutation_reports_the_value_it_replaced() {
        let prior = prior_state();
        let port = port_over(MapGlobals::seeded(&prior));
        let cause = EventId::new();

        let replaced = port.set("score", Variant::Int(42), false, cause).await;
        let incremented = port.increment("hits", 3, cause).await;
        let deleted = port.delete("temp", cause).await;

        assert_eq!(replaced, Ok(Some(prior[0].1.clone())));
        assert_eq!(
            incremented,
            Ok(GlobalIncrement {
                previous: Some(Variant::Int(10)),
                current: Variant::Int(13),
            })
        );
        assert_eq!(
            deleted,
            Ok(GlobalDeletion::Deleted {
                previous: Some(Variant::Float(2.5)),
            })
        );
    }

    #[tokio::test]
    async fn setting_a_new_key_reports_no_previous_value() {
        let port = port_over(MapGlobals::seeded(&[]));

        let replaced = port
            .set("fresh", Variant::Bool(true), false, EventId::new())
            .await;

        assert_eq!(replaced, Ok(None));
    }

    #[tokio::test]
    async fn an_unreadable_previous_value_does_not_block_set() {
        let globals = MapGlobals::seeded(&[(UNREADABLE, Variant::Int(1))]);
        let publisher = Arc::new(Recorder::default());
        let port = RuntimeScriptGlobals::new(
            Arc::clone(&globals) as Arc<dyn GlobalsRepo>,
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
        );

        let replaced = port
            .set(UNREADABLE, Variant::Int(7), false, EventId::new())
            .await;

        assert_eq!(replaced, Ok(None));
        assert_eq!(globals.stored(UNREADABLE), Some(Variant::Int(7)));
        let payloads: Vec<_> = publisher.events().into_iter().map(|e| e.payload).collect();
        assert_eq!(
            payloads,
            [serde_json::json!({ "key": UNREADABLE, "new_value": 7, "prev_value": null })]
        );
    }

    #[tokio::test]
    async fn an_unreadable_previous_value_does_not_block_delete() {
        let globals = MapGlobals::seeded(&[(UNREADABLE, Variant::Int(1))]);
        let publisher = Arc::new(Recorder::default());
        let port = RuntimeScriptGlobals::new(
            Arc::clone(&globals) as Arc<dyn GlobalsRepo>,
            Arc::clone(&publisher) as Arc<dyn EventPublisher>,
        );

        let deleted = port.delete(UNREADABLE, EventId::new()).await;

        assert_eq!(deleted, Ok(GlobalDeletion::Deleted { previous: None }));
        assert_eq!(globals.stored(UNREADABLE), None);
        let kinds: Vec<_> = publisher.events().into_iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["global.deleted"]);
    }
}
