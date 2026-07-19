use std::path::PathBuf;
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
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::FilePicker {
                key: "path",
                label: "File Path",
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

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "target_var".to_owned(),
                kind: VariantKind::String,
                label: "File contents".to_owned(),
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

        let abs_path = PathBuf::from(&interpolated_path);
        let (outcome, produced) = match tokio::fs::metadata(&abs_path).await {
            Ok(meta) if meta.len() > MAX_FILE_BYTES => (
                SubActionOutcome::Failed(format!(
                    "file exceeds {MAX_FILE_BYTES} byte cap: {} bytes",
                    meta.len()
                )),
                None,
            ),
            Ok(_) => match tokio::fs::read_to_string(&abs_path).await {
                Ok(contents) => {
                    let stack = ctx
                        .arg_stack
                        .clone()
                        .set(target_var, Variant::String(contents));
                    (SubActionOutcome::Success, Some(stack))
                }
                Err(e) => (SubActionOutcome::Failed(format!("read failed: {e}")), None),
            },
            Err(e) => (SubActionOutcome::Failed(format!("stat failed: {e}")), None),
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
            produced,
        )
    }
}
