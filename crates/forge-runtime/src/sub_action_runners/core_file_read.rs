use std::path::PathBuf;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

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
            FormField::FilePicker {
                key: "path",
                label: "File Path",
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
                    let value = match read_as.as_str() {
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
                Err(e) => (SubActionOutcome::Failed(format!("read failed: {e}")), None),
            },
            Err(e) => (SubActionOutcome::Failed(format!("stat failed: {e}")), None),
        };

        (timer.finish(outcome), produced)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    use super::*;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// Writes `contents` to a unique temp file, runs the reader under `read_as`
    /// (absent when `None`, storing into `target_var`), removes the file, and
    /// returns the outcome plus the value bound to `out_key` (if any).
    async fn read_with(
        read_as: Option<&str>,
        target_var: &str,
        out_key: &str,
        contents: &str,
    ) -> (SubActionOutcome, Option<Variant>) {
        let path = std::env::temp_dir().join(format!(
            "forge_read_{}_{}.dat",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        tokio::fs::write(&path, contents).await.unwrap();

        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String(path.to_string_lossy().into_owned()),
        );
        cfg.insert(
            "target_var".to_owned(),
            Variant::String(target_var.to_owned()),
        );
        if let Some(mode) = read_as {
            cfg.insert("read_as".to_owned(), Variant::String(mode.to_owned()));
        }

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (tel, produced) = CoreFileReadRunner.execute(&cfg, &ctx).await;
        let _ = tokio::fs::remove_file(&path).await;

        let value = produced.and_then(|s| s.get(out_key).cloned());
        (tel.outcome, value)
    }

    fn strings(items: &[&str]) -> Variant {
        Variant::Array(
            items
                .iter()
                .map(|s| Variant::String((*s).to_owned()))
                .collect(),
        )
    }

    #[tokio::test]
    async fn lines_mode_splits_on_newlines_stripping_trailing_carriage_returns() {
        let (outcome, value) =
            read_with(Some("Lines array"), "out", "out", "alpha\r\nbeta\ngamma").await;
        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(value, Some(strings(&["alpha", "beta", "gamma"])));
    }

    #[tokio::test]
    async fn missing_and_unknown_read_as_both_default_to_lines_array() {
        for read_as in [None, Some("something-else")] {
            let (outcome, value) = read_with(read_as, "out", "out", "x\ny").await;
            assert_eq!(outcome, SubActionOutcome::Success, "read_as={read_as:?}");
            assert_eq!(value, Some(strings(&["x", "y"])), "read_as={read_as:?}");
        }
    }

    #[tokio::test]
    async fn whole_file_mode_returns_the_entire_content_as_one_string() {
        let (outcome, value) = read_with(Some("Whole file"), "out", "out", "line1\nline2\n").await;
        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(value, Some(Variant::String("line1\nline2\n".to_owned())));
    }

    #[tokio::test]
    async fn json_mode_parses_object_root_into_variant_object() {
        let (outcome, value) =
            read_with(Some("JSON"), "out", "out", r#"{"name":"forge","count":3}"#).await;
        assert_eq!(outcome, SubActionOutcome::Success);
        let Some(Variant::Object(map)) = value else {
            panic!("expected Variant::Object, got {value:?}");
        };
        assert_eq!(map.get("name"), Some(&Variant::String("forge".to_owned())));
        assert_eq!(map.get("count"), Some(&Variant::Int(3)));
    }

    #[tokio::test]
    async fn json_mode_failures_report_typed_message_and_produce_no_binding() {
        // Malformed text fails at serde; a bare `null` parses but is an
        // unsupported Variant root. Both fail with no produced stack.
        for (contents, needle) in [
            ("{not valid json", "invalid JSON"),
            ("null", "unsupported JSON value"),
        ] {
            let (outcome, value) = read_with(Some("JSON"), "out", "out", contents).await;
            assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.contains(needle)),
                "contents {contents:?} expected {needle:?}, got {outcome:?}",
            );
            assert!(
                value.is_none(),
                "a failed JSON read must not bind a variable"
            );
        }
    }

    #[tokio::test]
    async fn missing_file_fails_with_stat_error_and_no_binding() {
        let path = std::env::temp_dir().join(format!(
            "forge_absent_{}_{}.dat",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String(path.to_string_lossy().into_owned()),
        );
        cfg.insert("target_var".to_owned(), Variant::String("out".to_owned()));

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (tel, produced) = CoreFileReadRunner.execute(&cfg, &ctx).await;

        assert!(
            matches!(&tel.outcome, SubActionOutcome::Failed(m) if m.contains("stat failed")),
            "got {:?}",
            tel.outcome,
        );
        assert!(produced.is_none());
    }

    #[tokio::test]
    async fn target_var_name_is_sanitized_before_binding() {
        // A `%result%` target name is stored under the bare `result` key, proving
        // the runner routes the name through strip_var_decoration.
        let (outcome, value) = read_with(Some("Whole file"), "  %result%  ", "result", "hi").await;
        assert_eq!(outcome, SubActionOutcome::Success);
        assert_eq!(value, Some(Variant::String("hi".to_owned())));
    }
}
