use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::os_ports::ClipboardPort;

const TEXT: TextField = TextField {
    key: "text",
    label: "Text",
    editor: TextEditor::Area,
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TEXT]);

pub struct CoreClipboardCopyRunner {
    clipboard: Arc<dyn ClipboardPort>,
}

impl CoreClipboardCopyRunner {
    pub fn new(clipboard: Arc<dyn ClipboardPort>) -> Self {
        Self { clipboard }
    }
}

#[async_trait]
impl SubActionRunner for CoreClipboardCopyRunner {
    fn id(&self) -> &str {
        "core.clipboard.copy"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Copy to Clipboard"
    }

    fn summary(&self) -> &str {
        "Copy text to the system clipboard"
    }

    fn search_text(&self) -> &str {
        "clipboard copy text paste system"
    }

    fn icon_name(&self) -> &str {
        "clipboard"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let text = match TEXT.read(config, ctx.arg_stack) {
            Ok(text) => text,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let clipboard = Arc::clone(&self.clipboard);
        let outcome = match tokio::task::spawn_blocking(move || clipboard.copy(text)).await {
            Ok(Ok(())) => SubActionOutcome::Success,
            Ok(Err(e)) => SubActionOutcome::Failed(e.to_string()),
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_action_runners::os_ports::test_ports::{
        MockErr, NullPublisher, RecordingClipboardPort,
    };
    use forge_types::EventId;

    async fn run(
        clipboard: Arc<RecordingClipboardPort>,
        stack: ArgStack,
        text: &str,
    ) -> SubActionOutcome {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(text.to_owned()));
        let publisher = NullPublisher;
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        CoreClipboardCopyRunner::new(clipboard)
            .execute(&cfg, &ctx)
            .await
            .0
            .outcome
    }

    #[tokio::test]
    async fn writes_interpolated_text_to_port() {
        let stack = ArgStack::new().set("name".to_owned(), Variant::String("World".to_owned()));
        let port = Arc::new(RecordingClipboardPort::new());
        let outcome = run(Arc::clone(&port), stack, "Hello %name%").await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(port.written(), vec!["Hello World".to_owned()]);
    }

    #[tokio::test]
    async fn maps_port_error_to_failed_without_panicking() {
        let port = Arc::new(RecordingClipboardPort::new().copy_fails(MockErr::Failed));
        let outcome = run(Arc::clone(&port), ArgStack::new(), "x").await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(port.written(), vec!["x".to_owned()], "copy was attempted");
    }
}
