use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_INTO_VAR)
            .to_owned();

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

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.clipboard.read".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
        )
    }
}
