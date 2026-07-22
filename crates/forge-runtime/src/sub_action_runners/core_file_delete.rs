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
                        SubActionOutcome::Failed(format!("file not found: {interpolated_path}"))
                    }
                }
                Err(e) => SubActionOutcome::Failed(format!("delete failed: {e}")),
            },
        };

        (timer.finish(outcome), None)
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
