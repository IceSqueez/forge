use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant, VariantKind,
};

use super::os_ports::ClipboardPort;

const DEFAULT_INTO_VAR: &str = "clipboard.text";

pub struct CoreClipboardReadRunner {
    clipboard: Arc<dyn ClipboardPort>,
}

impl CoreClipboardReadRunner {
    pub fn new(clipboard: Arc<dyn ClipboardPort>) -> Self {
        Self { clipboard }
    }
}

#[async_trait]
impl SubActionRunner for CoreClipboardReadRunner {
    fn id(&self) -> &str {
        "core.clipboard.read"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Read Clipboard"
    }

    fn summary(&self) -> &str {
        "Read the system clipboard into a variable"
    }

    fn search_text(&self) -> &str {
        "clipboard read paste text into variable system"
    }

    fn icon_name(&self) -> &str {
        "clipboard-text"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "into_var".to_owned(),
            Variant::String(DEFAULT_INTO_VAR.to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "into_var",
            label: "Output Variable",
            placeholder: DEFAULT_INTO_VAR,
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Clipboard text".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.clipboard.read");

        let into_var = super::interpolate::sanitize_var_name(
            config.str_nonempty("into_var").unwrap_or(DEFAULT_INTO_VAR),
        );

        let clipboard = Arc::clone(&self.clipboard);
        let (outcome, updated_stack) =
            match tokio::task::spawn_blocking(move || clipboard.read()).await {
                Ok(Ok(text)) => {
                    let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(text));
                    (SubActionOutcome::Success, Some(new_stack))
                }
                Ok(Err(e)) => (SubActionOutcome::Failed(e.to_string()), None),
                Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
            };

        (timer.finish(outcome), updated_stack)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_action_runners::os_ports::test_ports::{
        MockErr, NullPublisher, RecordingClipboardPort,
    };
    use forge_types::EventId;

    async fn run(
        clipboard: Arc<RecordingClipboardPort>,
        into_var: &str,
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let mut cfg = SubActionConfig::new();
        cfg.insert("into_var".to_owned(), Variant::String(into_var.to_owned()));
        let stack = ArgStack::new();
        let publisher = NullPublisher;
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        let (telemetry, updated) = CoreClipboardReadRunner::new(clipboard)
            .execute(&cfg, &ctx)
            .await;
        (telemetry.outcome, updated)
    }

    #[tokio::test]
    async fn writes_clipboard_value_into_named_var() {
        let port = Arc::new(RecordingClipboardPort::new().reads("hello"));
        let (outcome, updated) = run(port, "my.var").await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let stack = updated.unwrap();
        assert_eq!(stack.get("my.var").and_then(|v| v.as_str()), Some("hello"));
    }

    #[tokio::test]
    async fn empty_buffer_succeeds_with_empty_string() {
        // Contract: an accessible-but-empty clipboard is Success with an empty value,
        // NOT a failure.
        let port = Arc::new(RecordingClipboardPort::new().reads(""));
        let (outcome, updated) = run(port, "my.var").await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            updated.unwrap().get("my.var").and_then(|v| v.as_str()),
            Some("")
        );
    }

    #[tokio::test]
    async fn unavailable_clipboard_maps_to_failed_and_yields_no_stack() {
        let port = Arc::new(RecordingClipboardPort::new().read_fails(MockErr::Unavailable));
        let (outcome, updated) = run(port, "my.var").await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(
            updated.is_none(),
            "a failed read must not mutate the arg stack"
        );
    }

    #[tokio::test]
    async fn blank_into_var_falls_back_to_default_key() {
        let port = Arc::new(RecordingClipboardPort::new().reads("data"));
        let (outcome, updated) = run(port, "").await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            updated
                .unwrap()
                .get("clipboard.text")
                .and_then(|v| v.as_str()),
            Some("data")
        );
    }
}
