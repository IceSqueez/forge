use std::path::{Component, PathBuf};

use forge_events::{Event, EventSource};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{
    ArgStack, EventId, SubActionOutcome, SubActionSpec, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;

use crate::EventBus;

const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MiB sandbox cap

/// Returns the absolute on-disk path under the assets sandbox after rejecting
/// any traversal attempt. `Err(reason)` means the request must be refused.
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

pub(super) async fn run(
    spec: &SubActionSpec,
    arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    dp: &dyn DataProvider,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::ReadFile { path, target_var } = spec else {
        unreachable!()
    };

    let interpolated_path = super::interpolate_with_globals(path, arg_stack, dp).await;
    let target_var = target_var.clone();

    let outcome = match resolve_sandboxed(&interpolated_path) {
        Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
        Ok(abs_path) => match tokio::fs::metadata(&abs_path).await {
            Ok(meta) if meta.len() > MAX_FILE_BYTES => SubActionOutcome::Failed(format!(
                "file exceeds {MAX_FILE_BYTES} byte cap: {} bytes",
                meta.len()
            )),
            Ok(_) => match tokio::fs::read_to_string(&abs_path).await {
                Ok(contents) => {
                    match GlobalsRepo::set(dp, &target_var, Variant::String(contents), false).await
                    {
                        Ok(()) => {
                            bus.publish(Event::caused_by(
                                EventSource::Core,
                                "global.set",
                                serde_json::json!({
                                    "key": target_var,
                                    "source": "read_file",
                                    "path": interpolated_path,
                                }),
                                parent_event_id,
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

    SubActionTelemetry {
        index,
        kind: "ReadFile".to_string(),
        started_at,
        duration_ms,
        outcome,
    }
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
