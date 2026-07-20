use async_trait::async_trait;
use base64::Engine as _;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt as _;

pub struct CoreFileWriteRunner;

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

        let interpolated_path = ctx.arg_stack.interpolate(path_template);
        let content = ctx.arg_stack.interpolate(content_template);

        let (outcome, updated) = match super::file_sandbox::resolve_sandboxed(&interpolated_path) {
            Err(reason) => (
                SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
                None,
            ),
            Ok(abs_path) => {
                match do_write(&abs_path, &content, &encoding, &mode, create_parent_dirs).await {
                    Err(reason) => (SubActionOutcome::Failed(reason), None),
                    Ok(bytes_written) => {
                        let stack = ctx.arg_stack.clone().set(
                            "file.bytes_written".to_owned(),
                            Variant::Int(bytes_written as i64),
                        );
                        (SubActionOutcome::Success, Some(stack))
                    }
                }
            }
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.file.write".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated,
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    #[test]
    fn encode_content_utf8_returns_raw_utf8_bytes() {
        // Empty encoding string aliases utf8 - both must yield identical bytes.
        let expected = "héllo".as_bytes().to_vec();
        assert_eq!(encode_content("héllo", "utf8").unwrap(), expected);
        assert_eq!(encode_content("héllo", "").unwrap(), expected);
    }

    #[test]
    fn encode_content_latin1_maps_each_char_to_one_byte() {
        // 'A' = 0x41, 'ÿ' (U+00FF) = 0xFF.
        assert_eq!(encode_content("Aÿ", "latin1").unwrap(), vec![0x41, 0xFF]);
    }

    #[test]
    fn encode_content_latin1_accepts_ff_boundary_and_rejects_above() {
        // U+00FF is the highest codepoint with a Latin-1 byte; U+0100 is the first without.
        assert_eq!(encode_content("\u{00FF}", "latin1").unwrap(), vec![0xFF]);
        let err = encode_content("\u{0100}", "latin1").unwrap_err();
        assert!(err.contains("Latin-1"), "{err}");
    }

    #[test]
    fn encode_content_base64_decodes_trimmed_input() {
        // base64("hi") == "aGk="; surrounding whitespace is trimmed before decode.
        assert_eq!(encode_content("aGk=", "raw_base64").unwrap(), b"hi");
        assert_eq!(encode_content("  aGk=\n", "raw_base64").unwrap(), b"hi");
    }

    #[test]
    fn encode_content_base64_rejects_invalid_payload() {
        let err = encode_content("not base64!", "raw_base64").unwrap_err();
        assert!(err.contains("base64 decode failed"), "{err}");
    }

    #[test]
    fn encode_content_rejects_unknown_encoding() {
        let err = encode_content("x", "utf16").unwrap_err();
        assert!(err.contains("unknown encoding"), "{err}");
    }

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    // Proves resolve_sandboxed is wired BEFORE any fs write: a traversal path
    // yields the sandbox-rejection outcome and binds no scope variable. If the
    // guard were skipped, the path would reach tokio::fs and produce a different
    // ("open failed") message instead.
    #[tokio::test]
    async fn write_rejects_parent_traversal_before_touching_disk() {
        let runner = CoreFileWriteRunner;
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String("../escape.txt".to_owned()),
        );
        cfg.insert("content".to_owned(), Variant::String("data".to_owned()));

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
            "no scope variable must be bound when the sandbox rejects the path"
        );
    }
}
