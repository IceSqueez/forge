use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use time::OffsetDateTime;

pub struct CoreGlobalsArrayRemoveRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsArrayRemoveRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsArrayRemoveRunner {
    fn id(&self) -> &str {
        "core.globals.array_remove"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Remove from Global Array"
    }

    fn summary(&self) -> &str {
        "Remove matching items from a global array variable"
    }

    fn search_text(&self) -> &str {
        "remove delete array global list item filter"
    }

    fn icon_name(&self) -> &str {
        "list-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg.insert("remove_all".to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "key",
                label: "Variable Name",
                placeholder: "my_list",
            },
            FormField::Text {
                key: "value",
                label: "Value to Remove",
                placeholder: "item",
            },
            FormField::Toggle {
                key: "remove_all",
                label: "Remove All Matches",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.array_remove: key is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let key_template = config
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let value_template = config
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let remove_all = config
            .get("remove_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let resolved_key = super::interpolate::interpolate_with_globals(
            key_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let raw_value = super::interpolate::interpolate_with_globals(
            value_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let target = super::interpolate::parse_variant(&raw_value);

        let outcome = match self.globals.get(&resolved_key).await {
            Err(e) => SubActionOutcome::Failed(e.to_string()),
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.globals.array_remove: global '{}' does not exist",
                resolved_key
            )),
            Ok(Some(Variant::Array(mut arr))) => {
                if remove_all {
                    arr.retain(|item| item != &target);
                } else {
                    // Remove only the first matching element (leftmost).
                    if let Some(pos) = arr.iter().position(|item| item == &target) {
                        arr.remove(pos);
                    }
                }

                let new_len = arr.len();
                let persisted = self
                    .globals
                    .persisted(&resolved_key)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false);
                match self
                    .globals
                    .set(&resolved_key, Variant::Array(arr), persisted)
                    .await
                {
                    Ok(()) => {
                        ctx.publisher.publish(Event::caused_by(
                            EventSource::Core,
                            "global.set",
                            serde_json::json!({
                                "key": resolved_key,
                                "new_length": new_len,
                            }),
                            ctx.parent_event_id,
                        ));
                        SubActionOutcome::Success
                    }
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                }
            }
            Ok(Some(other)) => SubActionOutcome::Failed(format!(
                "core.globals.array_remove: expected array, found {}",
                VariantKind::from_variant(&other).label()
            )),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.array_remove".to_owned(),
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
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[derive(Default)]
    struct MapGlobals {
        map: Mutex<BTreeMap<String, Variant>>,
    }

    impl MapGlobals {
        fn with(entries: impl IntoIterator<Item = (&'static str, Variant)>) -> Self {
            let map = entries
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
            Self {
                map: Mutex::new(map),
            }
        }
        fn array(&self, key: &str) -> Vec<Variant> {
            match self.map.lock().unwrap().get(key) {
                Some(Variant::Array(a)) => a.clone(),
                other => panic!("expected array at {key}, got {other:?}"),
            }
        }
    }

    #[async_trait]
    impl GlobalsRepo for MapGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).cloned())
        }
        async fn set(&self, name: &str, value: Variant, _p: bool) -> Result<(), StorageError> {
            self.map.lock().unwrap().insert(name.to_owned(), value);
            Ok(())
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

    fn cfg(key: &str, value: &str, remove_all: bool) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("key".to_owned(), Variant::String(key.to_owned()));
        c.insert("value".to_owned(), Variant::String(value.to_owned()));
        c.insert("remove_all".to_owned(), Variant::Bool(remove_all));
        c
    }

    /// [1, 2, 1, 3, 1] — the value `1` appears three times so first-only and
    /// remove-all produce distinguishable results.
    fn seeded() -> Arc<MapGlobals> {
        Arc::new(MapGlobals::with([(
            "list",
            Variant::Array(vec![
                Variant::Int(1),
                Variant::Int(2),
                Variant::Int(1),
                Variant::Int(3),
                Variant::Int(1),
            ]),
        )]))
    }

    async fn run(globals: Arc<MapGlobals>, config: &SubActionConfig) -> SubActionOutcome {
        let runner = CoreGlobalsArrayRemoveRunner::new(globals);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn array_remove_first_only_drops_leftmost_match() {
        let globals = seeded();
        let outcome = run(globals.clone(), &cfg("list", "1", false)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        // Only the leftmost Int(1) is gone; later duplicates survive.
        assert_eq!(
            globals.array("list"),
            vec![
                Variant::Int(2),
                Variant::Int(1),
                Variant::Int(3),
                Variant::Int(1),
            ]
        );
    }

    #[tokio::test]
    async fn array_remove_all_drops_every_match() {
        let globals = seeded();
        let outcome = run(globals.clone(), &cfg("list", "1", true)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            globals.array("list"),
            vec![Variant::Int(2), Variant::Int(3)]
        );
    }

    #[tokio::test]
    async fn array_remove_absent_value_leaves_array_unchanged() {
        let globals = seeded();
        let outcome = run(globals.clone(), &cfg("list", "99", false)).await;
        // Absent value is a no-op success, NOT a failure.
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            globals.array("list"),
            vec![
                Variant::Int(1),
                Variant::Int(2),
                Variant::Int(1),
                Variant::Int(3),
                Variant::Int(1),
            ]
        );
    }

    #[tokio::test]
    async fn array_remove_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals, &cfg("ghost", "1", false)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn array_remove_non_array_global_reports_failed() {
        let globals = Arc::new(MapGlobals::with([("list", Variant::Int(5))]));
        let outcome = run(globals, &cfg("list", "1", false)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }
}
