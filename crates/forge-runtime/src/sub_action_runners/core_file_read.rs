use std::path::{Component, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

const MAX_FILE_BYTES: u64 = 1_048_576;

pub struct CoreFileReadRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreFileReadRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreFileReadRunner {
    fn id(&self) -> &str {
        "core.file.read"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Read File"
    }

    fn summary(&self) -> &str {
        "Read a text file from the assets sandbox into a global variable"
    }

    fn search_text(&self) -> &str {
        "read file text load assets"
    }

    fn icon_name(&self) -> &str {
        "file-text"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String(String::new()));
        cfg.insert("target_var".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "path",
                label: "File Path (relative to assets/)",
                placeholder: "greeting.txt",
            },
            FormField::Text {
                key: "target_var",
                label: "Target Variable",
                placeholder: "file_contents",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let path_ok = config
            .get("path")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let var_ok = config
            .get("target_var")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if path_ok && var_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.file.read: path and target_var are required".to_owned(),
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
        let target_var = config
            .get("target_var")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();

        let interpolated_path = super::interpolate::interpolate_with_globals(
            path_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match resolve_sandboxed(&interpolated_path) {
            Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
            Ok(abs_path) => match tokio::fs::metadata(&abs_path).await {
                Ok(meta) if meta.len() > MAX_FILE_BYTES => SubActionOutcome::Failed(format!(
                    "file exceeds {MAX_FILE_BYTES} byte cap: {} bytes",
                    meta.len()
                )),
                Ok(_) => match tokio::fs::read_to_string(&abs_path).await {
                    Ok(contents) => {
                        match self
                            .globals
                            .set(&target_var, Variant::String(contents), false)
                            .await
                        {
                            Ok(()) => {
                                ctx.publisher.publish(Event::caused_by(
                                    EventSource::Core,
                                    "global.set",
                                    serde_json::json!({
                                        "key": target_var,
                                        "source": "read_file",
                                        "path": interpolated_path,
                                    }),
                                    ctx.parent_event_id,
                                ));
                                SubActionOutcome::Success
                            }
                            Err(e) => SubActionOutcome::Failed(format!("global write failed: {e}")),
                        }
                    }
                    Err(e) => SubActionOutcome::Failed(format!("read failed: {e}")),
                },
                Err(e) => SubActionOutcome::Failed(format!("stat failed: {e}")),
            },
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.file.read".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn resolve_sandboxed(rel: &str) -> Result<PathBuf, String> {
    if rel.is_empty() {
        return Err("path is empty".to_owned());
    }
    if rel.starts_with('/') || rel.starts_with('\\') {
        return Err("absolute paths are forbidden".to_owned());
    }
    let candidate = PathBuf::from(rel);
    for component in candidate.components() {
        match component {
            Component::ParentDir => return Err("parent dir traversal forbidden".to_owned()),
            Component::Prefix(_) | Component::RootDir => {
                return Err("rooted paths are forbidden".to_owned());
            }
            _ => {}
        }
    }
    let root = forge_platform_core::paths::data_dir().join("assets");
    Ok(root.join(candidate))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::resolve_sandboxed;

    #[test]
    fn rejects_absolute_path() {
        assert!(resolve_sandboxed("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        assert!(resolve_sandboxed("../etc/passwd").is_err());
        assert!(resolve_sandboxed("foo/../../bar").is_err());
    }

    #[test]
    fn accepts_simple_relative_path() {
        let p = resolve_sandboxed("greeting.txt").unwrap();
        assert!(p.ends_with("assets/greeting.txt"));
    }

    #[test]
    fn accepts_nested_relative_path() {
        let p = resolve_sandboxed("subdir/file.txt").unwrap();
        assert!(p.ends_with("assets/subdir/file.txt"));
    }

    #[test]
    fn rejects_empty_path() {
        assert!(resolve_sandboxed("").is_err());
    }
}
