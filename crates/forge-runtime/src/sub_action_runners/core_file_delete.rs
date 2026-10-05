use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreFileDeleteRunner;

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
                label: "File Path (relative to the files folder)",
                placeholder: "output/data.txt",
            },
            FormField::Toggle {
                key: "ignore_missing",
                label: "Ignore Missing File",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("path").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.file.delete");

        let path_template = config.str("path").unwrap_or_default();
        let ignore_missing = config.bool("ignore_missing").unwrap_or(false);

        let interpolated_path = ctx.arg_stack.interpolate(path_template);

        let outcome = match super::file_sandbox::resolve_sandboxed(&interpolated_path).await {
            Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
            Ok(abs_path) => match tokio::fs::remove_file(&abs_path).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if ignore_missing {
                        SubActionOutcome::Success
                    } else {
                        SubActionOutcome::Failed("core.file.delete: file not found".to_owned())
                    }
                }
                Err(e) => SubActionOutcome::Failed(format!("delete failed: {e}")),
            },
        };

        (timer.finish(outcome), None)
    }
}
