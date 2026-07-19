use std::path::Path;
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
use time::OffsetDateTime;

pub struct CoreFileListRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreFileListRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreFileListRunner {
    fn id(&self) -> &str {
        "core.file.list"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "List Directory"
    }

    fn summary(&self) -> &str {
        "Enumerate a sandboxed directory; stores entry names in a variable array"
    }

    fn search_text(&self) -> &str {
        "list directory files glob recursive assets"
    }

    fn icon_name(&self) -> &str {
        "folder"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String(String::new()));
        cfg.insert("pattern".to_owned(), Variant::String("*".to_owned()));
        cfg.insert("recursive".to_owned(), Variant::Bool(false));
        cfg.insert("include_dirs".to_owned(), Variant::Bool(false));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("file.entries".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "path",
                label: "Directory Path (relative to assets/)",
                placeholder: "logs/",
            },
            FormField::Text {
                key: "pattern",
                label: "Glob Pattern",
                placeholder: "*",
            },
            FormField::Toggle {
                key: "recursive",
                label: "Recursive",
            },
            FormField::Toggle {
                key: "include_dirs",
                label: "Include Directories",
            },
            FormField::Text {
                key: "into_var",
                label: "Target Variable",
                placeholder: "file.entries",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let path_ok = config
            .get("path")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let var_ok = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if path_ok && var_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.file.list: path and into_var are required".to_owned(),
            ))
        }
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Array,
                label: "Directory entries".to_owned(),
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

        let path_template = config
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let pattern = config
            .get("pattern")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("*")
            .to_owned();
        let recursive = config
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let include_dirs = config
            .get("include_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("file.entries")
            .to_owned();

        let interpolated_path = super::interpolate::interpolate_with_globals(
            path_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let (outcome, produced) = match super::file_sandbox::resolve_sandboxed(&interpolated_path) {
            Err(reason) => (
                SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
                None,
            ),
            Ok(abs_path) => match tokio::fs::metadata(&abs_path).await {
                Err(_) => (
                    SubActionOutcome::Failed(format!("directory not found: {interpolated_path}")),
                    None,
                ),
                Ok(meta) if !meta.is_dir() => (
                    SubActionOutcome::Failed(format!("not a directory: {interpolated_path}")),
                    None,
                ),
                Ok(_) => {
                    match collect_entries(&abs_path, &pattern, recursive, include_dirs).await {
                        Err(reason) => (SubActionOutcome::Failed(reason), None),
                        Ok(entries) => {
                            let array =
                                Variant::Array(entries.into_iter().map(Variant::String).collect());
                            let stack = ctx.arg_stack.clone().set(into_var, array);
                            (SubActionOutcome::Success, Some(stack))
                        }
                    }
                }
            },
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.file.list".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            produced,
        )
    }
}

/// Iterative BFS walk to avoid stack overflow on deep directory trees.
/// Entry paths are relative to `root` and use `/` as separator on all platforms.
async fn collect_entries(
    root: &Path,
    pattern: &str,
    recursive: bool,
    include_dirs: bool,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut read_dir = tokio::fs::read_dir(&current)
            .await
            .map_err(|e| format!("read_dir failed: {e}"))?;

        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| format!("read entry failed: {e}"))?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|e| format!("file_type failed: {e}"))?;

            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = entry
                .path()
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());

            if file_type.is_dir() {
                if recursive {
                    stack.push(entry.path());
                }
                if include_dirs && super::file_sandbox::glob_matches(pattern, &name) {
                    result.push(rel);
                }
            } else if super::file_sandbox::glob_matches(pattern, &name) {
                result.push(rel);
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

    #[derive(Default)]
    struct RecordingGlobals {
        writes: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl GlobalsRepo for RecordingGlobals {
        async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(None)
        }
        async fn set(&self, name: &str, _value: Variant, _p: bool) -> Result<(), StorageError> {
            self.writes.lock().unwrap().push(name.to_owned());
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
    // reach tokio::fs::metadata (which would yield a "directory not found"
    // message), and must produce no scope stack. The empty-`writes` assertion
    // guards against the output ever resurfacing as a globals write.
    #[tokio::test]
    async fn list_rejects_parent_traversal_before_touching_disk() {
        let globals = Arc::new(RecordingGlobals::default());
        let runner = CoreFileListRunner::new(globals.clone());
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String("../".to_owned()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("file.entries".to_owned()),
        );

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(&cfg, &ctx).await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if msg.contains("sandbox rejected")),
            "expected sandbox rejection, got {:?}",
            telemetry.outcome
        );
        assert!(
            produced.is_none(),
            "a rejected path must not produce a scope stack"
        );
        assert!(
            globals.writes.lock().unwrap().is_empty(),
            "no global must be written when the sandbox rejects the path"
        );
    }
}
