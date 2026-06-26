use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::os_ports::ClipboardPort;

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("text".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "text",
            label: "Text",
        }]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let text = ctx
            .arg_stack
            .interpolate(config.get("text").and_then(|v| v.as_str()).unwrap_or(""));

        let clipboard = Arc::clone(&self.clipboard);
        let outcome = match tokio::task::spawn_blocking(move || clipboard.copy(text)).await {
            Ok(Ok(())) => SubActionOutcome::Success,
            Ok(Err(e)) => SubActionOutcome::Failed(e.to_string()),
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.clipboard.copy".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}
