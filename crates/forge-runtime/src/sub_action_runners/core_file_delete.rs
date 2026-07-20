use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
                label: "File Path (relative to assets/)",
                placeholder: "output/data.txt",
            },
            FormField::Toggle {
                key: "ignore_missing",
                label: "Ignore Missing File",
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
                "core.file.delete: path is required".to_owned(),
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
        let ignore_missing = config
            .get("ignore_missing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let interpolated_path = ctx.arg_stack.interpolate(path_template);

        let outcome = match super::file_sandbox::resolve_sandboxed(&interpolated_path).await {
            Err(reason) => SubActionOutcome::Failed(format!("sandbox rejected path: {reason}")),
            Ok(abs_path) => match tokio::fs::remove_file(&abs_path).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if ignore_missing {
                        SubActionOutcome::Success
                    } else {
                        SubActionOutcome::Failed(format!("file not found: {interpolated_path}"))
                    }
                }
                Err(e) => SubActionOutcome::Failed(format!("delete failed: {e}")),
            },
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.file.delete".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    // A traversal path must surface the sandbox-rejection outcome rather than
    // reach tokio::fs::remove_file (which would yield a "file not found" message).
    #[tokio::test]
    async fn delete_rejects_parent_traversal_before_touching_disk() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String("../../etc/passwd".to_owned()),
        );

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let outcome = CoreFileDeleteRunner.execute(&cfg, &ctx).await.0.outcome;

        assert!(
            matches!(&outcome, SubActionOutcome::Failed(msg) if msg.contains("sandbox rejected")),
            "expected sandbox rejection, got {outcome:?}"
        );
    }

    // RFC-101: field interpolation is scope-only. A `%name%` present in the arg
    // stack resolves; a token absent from the stack has no globals fallthrough
    // and stays verbatim. Both appear in the observable "file not found" path,
    // so the contract is proven behaviorally rather than via a private API.
    #[tokio::test]
    async fn delete_resolves_scope_token_and_leaves_unknown_token_verbatim() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "path".to_owned(),
            Variant::String("%dir%/%unset_global%.txt".to_owned()),
        );

        let stack = ArgStack::new().set("dir".to_owned(), Variant::String("sub".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let outcome = CoreFileDeleteRunner.execute(&cfg, &ctx).await.0.outcome;

        let SubActionOutcome::Failed(msg) = outcome else {
            panic!("expected file-not-found failure, got {outcome:?}");
        };
        assert!(
            msg.contains("sub/"),
            "scope token was not interpolated: {msg}"
        );
        assert!(
            msg.contains("%unset_global%"),
            "unknown token must stay verbatim (no globals fallthrough): {msg}"
        );
    }
}
