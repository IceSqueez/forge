use std::path::Path;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

pub struct CoreFileListRunner;

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
                label: "Output Variable",
                placeholder: "file.entries",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config
            .require_str("path")
            .and(config.require_str("into_var"))
            .map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::Array,
                label: "Directory entries".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.file.list");

        let path_template = config.str("path").unwrap_or_default();
        let pattern = config.str_nonempty("pattern").unwrap_or("*").to_owned();
        let recursive = config.bool("recursive").unwrap_or(false);
        let include_dirs = config.bool("include_dirs").unwrap_or(false);
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("file.entries"),
        );

        let interpolated_path = ctx.arg_stack.interpolate(path_template);

        let (outcome, produced) = match super::file_sandbox::resolve_sandboxed(&interpolated_path)
            .await
        {
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

        (timer.finish(outcome), produced)
    }
}

/// Iterative (not recursive) to avoid stack overflow on deep directory trees.
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
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[tokio::test]
    async fn list_rejects_parent_traversal_before_touching_disk() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String("../".to_owned()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("file.entries".to_owned()),
        );

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = CoreFileListRunner.execute(&cfg, &ctx).await;

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(msg) if msg.contains("sandbox rejected")),
            "expected sandbox rejection, got {:?}",
            telemetry.outcome
        );
        assert!(
            produced.is_none(),
            "a rejected path must not produce a scope stack"
        );
    }
}
