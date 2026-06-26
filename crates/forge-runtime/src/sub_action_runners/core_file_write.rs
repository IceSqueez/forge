use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt as _;

pub struct CoreFileWriteRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreFileWriteRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreFileWriteRunner {
    fn id(&self) -> &str {
        "core.file.write"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Files
    }

    fn label(&self) -> &str {
        "Write File"
    }

    fn summary(&self) -> &str {
        "Write content to a sandboxed file; stores bytes written in `file.bytes_written`"
    }

    fn search_text(&self) -> &str {
        "write file save output create append overwrite assets"
    }

    fn icon_name(&self) -> &str {
        "file-edit"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("path".to_owned(), Variant::String(String::new()));
        cfg.insert("content".to_owned(), Variant::String(String::new()));
        cfg.insert("encoding".to_owned(), Variant::String("utf8".to_owned()));
        cfg.insert("mode".to_owned(), Variant::String("overwrite".to_owned()));
        cfg.insert("create_parent_dirs".to_owned(), Variant::Bool(false));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "path",
                label: "File Path (relative to assets/)",
                placeholder: "output/data.txt",
            },
            FormField::TextArea {
                key: "content",
                label: "Content",
            },
            FormField::Select {
                key: "encoding",
                label: "Encoding",
                options: &["utf8", "latin1", "raw_base64"],
            },
            FormField::Select {
                key: "mode",
                label: "Write Mode",
                options: &["overwrite", "append", "create_new"],
            },
            FormField::Toggle {
                key: "create_parent_dirs",
                label: "Create Parent Directories",
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
                "core.file.write: path is required".to_owned(),
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
        let content_template = config
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let encoding = config
            .get("encoding")
            .and_then(|v| v.as_str())
            .unwrap_or("utf8")
            .to_owned();
        let mode = config
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("overwrite")
            .to_owned();
        let create_parent_dirs = config
            .get("create_parent_dirs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let interpolated_path = super::interpolate::interpolate_with_globals(
            path_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;
        let content = super::interpolate::interpolate_with_globals(
            content_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match super::file_sandbox::resolve_sandboxed(&interpolated_path) {
            Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
            Ok(abs_path) => {
                match do_write(&abs_path, &content, &encoding, &mode, create_parent_dirs).await {
                    Err(reason) => SubActionOutcome::Failed(reason),
                    Ok(bytes_written) => {
                        match self
                            .globals
                            .set(
                                "file.bytes_written",
                                Variant::Int(bytes_written as i64),
                                false,
                            )
                            .await
                        {
                            Ok(()) => SubActionOutcome::Success,
                            Err(e) => {
                                SubActionOutcome::Failed(format!("store bytes_written failed: {e}"))
                            }
                        }
                    }
                }
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.file.write".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

fn encode_content(content: &str, encoding: &str) -> Result<Vec<u8>, String> {
    match encoding {
        "" | "utf8" => Ok(content.as_bytes().to_vec()),
        "latin1" => content
            .chars()
            .map(|c| {
                let code = c as u32;
                if code <= 0xFF {
                    Ok(code as u8)
                } else {
                    Err(format!("'{c}' (U+{code:04X}) has no Latin-1 encoding"))
                }
            })
            .collect(),
        "raw_base64" => base64::engine::general_purpose::STANDARD
            .decode(content.trim())
            .map_err(|e| format!("base64 decode failed: {e}")),
        other => Err(format!("unknown encoding: '{other}'")),
    }
}

async fn do_write(
    abs_path: &std::path::Path,
    content: &str,
    encoding: &str,
    mode: &str,
    create_parent_dirs: bool,
) -> Result<u64, String> {
    if create_parent_dirs && let Some(parent) = abs_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("create_dir_all failed: {e}"))?;
    }

    let bytes = encode_content(content, encoding)?;

    let mut opts = tokio::fs::OpenOptions::new();
    match mode {
        "" | "overwrite" => {
            opts.write(true).create(true).truncate(true);
        }
        "append" => {
            opts.append(true).create(true);
        }
        "create_new" => {
            opts.write(true).create_new(true);
        }
        other => return Err(format!("unknown write mode: '{other}'")),
    }

    let mut file = opts
        .open(abs_path)
        .await
        .map_err(|e| format!("open failed: {e}"))?;

    file.write_all(&bytes)
        .await
        .map_err(|e| format!("write failed: {e}"))?;

    Ok(bytes.len() as u64)
}
