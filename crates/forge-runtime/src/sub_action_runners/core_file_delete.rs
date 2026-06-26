use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreFileDeleteRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreFileDeleteRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreFileDeleteRunner {
    fn id(&self) -> &str {
        "core.file.delete"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Delete File"
    }

    fn summary(&self) -> &str {
        "Remove a sandboxed file; never removes directories"
    }

    fn search_text(&self) -> &str {
        "delete remove file assets"
    }

    fn icon_name(&self) -> &str {
        "file-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String(String::new()));
        cfg.insert("ignore_missing".to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "path",
                label: "File Path (relative to assets/)",
                placeholder: "output/data.txt",
            },
            FormField::Toggle {
                key: "ignore_missing",
                label: "Ignore Missing File",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let path_ok = config
            .get("path")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if path_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.file.delete: path is required".to_owned(),
            ))
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let path_template = config
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let ignore_missing = config
            .get("ignore_missing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let interpolated_path = super::interpolate::interpolate_with_globals(
            path_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match super::file_sandbox::resolve_sandboxed(&interpolated_path) {
            Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
            Ok(abs_path) => match tokio::fs::remove_file(&abs_path).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if ignore_missing {
                        SubActionOutcome::Success
                    } else {
                        SubActionOutcome::Failed(format!("file not found: {interpolated_path}"))
                    }
                }
                Err(e) => SubActionOutcome::Failed(format!("delete failed: {e}")),
            },
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.file.delete".to_owned(),
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
    use forge_events::{Event, EventPublisher};
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    struct StubGlobals;
    #[async_trait]
    impl GlobalsRepo for StubGlobals {
        async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(None)
        }
        async fn set(&self, _name: &str, _value: Variant, _p: bool) -> Result<(), StorageError> {
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

    // A traversal path must surface the sandbox-rejection outcome rather than
    // reach tokio::fs::remove_file (which would yield a "file not found" message).
    #[tokio::test]
    async fn delete_rejects_parent_traversal_before_touching_disk() {
        let runner = CoreFileDeleteRunner::new(Arc::new(StubGlobals));
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String("../../etc/passwd".to_owned()),
        );

        let stack = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        };
        let outcome = runner.execute(&cfg, &ctx).await.0.outcome;

        assert!(
            matches!(&outcome, SubActionOutcome::Failed(msg) if msg.contains("sandbox rejected")),
            "expected sandbox rejection, got {outcome:?}"
        );
    }
}
