use std::path::Path;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};
use tokio::io::AsyncReadExt;

const MAX_FILE_BYTES: u64 = 1_048_576;

pub struct CoreFileReadRunner;

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
        "Read a text file into a variable"
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
        cfg.insert(
            "read_as".to_owned(),
            Variant::String("Lines array".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "path",
                label: "File Path (relative to the files folder)",
                placeholder: "quotes/cards.md",
            },
            FormField::Select {
                key: "read_as",
                label: "Read As",
                options: &["Lines array", "Whole file", "JSON"],
            },
            FormField::Text {
                key: "target_var",
                label: "Output Variable",
                placeholder: "file_contents",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config
            .require_str("path")
            .and(config.require_str("target_var"))
            .map(|_| ())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "target_var".to_owned(),
                kind: VariantKind::Array,
                label: "File contents".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.file.read");

        let path_template = config.str("path").unwrap_or_default();
        let target_var =
            forge_types::strip_var_decoration(config.str("target_var").unwrap_or_default());
        let read_as = config.str("read_as").unwrap_or("Lines array").to_owned();

        let interpolated_path = ctx.arg_stack.interpolate(path_template);

        let (outcome, produced) =
            match super::file_sandbox::resolve_sandboxed(&interpolated_path).await {
                Err(reason) => (
                    SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
                    None,
                ),
                Ok(abs_path) => read_into(&abs_path, &read_as, target_var, ctx).await,
            };

        (timer.finish(outcome), produced)
    }
}

async fn read_into(
    abs_path: &Path,
    read_as: &str,
    target_var: String,
    ctx: &RunContext<'_>,
) -> (SubActionOutcome, Option<ArgStack>) {
    match tokio::fs::metadata(abs_path).await {
        Ok(meta) if meta.len() > MAX_FILE_BYTES => (
            SubActionOutcome::Failed(format!(
                "file exceeds {MAX_FILE_BYTES} byte cap: {} bytes",
                meta.len()
            )),
            None,
        ),
        Ok(meta) if !meta.is_file() => (
            SubActionOutcome::Failed("path is not a file".to_owned()),
            None,
        ),
        Ok(_) => match read_capped(abs_path).await {
            Ok(contents) => {
                let value = match read_as {
                    "Whole file" => Ok(Variant::String(contents)),
                    "JSON" => serde_json::from_str::<serde_json::Value>(&contents)
                        .map_err(|e| format!("invalid JSON: {e}"))
                        .and_then(|json| {
                            Variant::from_json(json)
                                .map_err(|e| format!("unsupported JSON value: {e}"))
                        }),
                    _ => Ok(Variant::Array(
                        contents
                            .lines()
                            .map(|line| Variant::String(line.to_owned()))
                            .collect(),
                    )),
                };
                match value {
                    Ok(value) => {
                        let stack = ctx.arg_stack.clone().set(target_var, value);
                        (SubActionOutcome::Success, Some(stack))
                    }
                    Err(e) => (SubActionOutcome::Failed(e), None),
                }
            }
            Err(e) => (SubActionOutcome::Failed(e), None),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            SubActionOutcome::Failed("file not found in the files folder".to_owned()),
            None,
        ),
        Err(e) => (SubActionOutcome::Failed(format!("stat failed: {e}")), None),
    }
}

async fn read_capped(path: &Path) -> Result<String, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("read failed: {e}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("file exceeds {MAX_FILE_BYTES} byte cap"));
    }
    String::from_utf8(bytes).map_err(|e| format!("read failed: {e}"))
}
