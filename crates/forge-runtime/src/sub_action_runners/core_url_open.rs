use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::os_ports::UrlOpenPort;

pub struct CoreUrlOpenRunner {
    opener: Arc<dyn UrlOpenPort>,
}

impl CoreUrlOpenRunner {
    pub fn new(opener: Arc<dyn UrlOpenPort>) -> Self {
        Self { opener }
    }
}

#[async_trait]
impl SubActionRunner for CoreUrlOpenRunner {
    fn id(&self) -> &str {
        "core.url.open"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Open URL"
    }

    fn summary(&self) -> &str {
        "Open a URL in the default browser"
    }

    fn search_text(&self) -> &str {
        "url open browser link web http https"
    }

    fn icon_name(&self) -> &str {
        "external-link"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "url",
            label: "URL",
            placeholder: "https://example.com",
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

        let url = ctx
            .arg_stack
            .interpolate(config.get("url").and_then(|v| v.as_str()).unwrap_or(""));

        let outcome = if is_browser_scheme(&url) {
            let opener = Arc::clone(&self.opener);
            match tokio::task::spawn_blocking(move || opener.open(url)).await {
                Ok(Ok(())) => SubActionOutcome::Success,
                Ok(Err(e)) => SubActionOutcome::Failed(e.to_string()),
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        } else {
            SubActionOutcome::Failed(format!("rejected non-http(s) URL: {url}"))
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.url.open".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

/// Gate the interpolated URL to `http`/`https` before handing it to the OS
/// opener; interpolation may splice untrusted chat input that would otherwise
/// reach `file://`, `javascript:`, or `data:` handlers.
fn is_browser_scheme(url: &str) -> bool {
    match url.trim().split_once(':') {
        Some((scheme, _)) => matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https"),
        None => false,
    }
}
