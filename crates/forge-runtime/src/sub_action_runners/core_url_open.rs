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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_action_runners::os_ports::test_ports::{
        MockErr, NullPublisher, RecordingUrlOpenPort,
    };
    use forge_types::EventId;

    async fn run(
        opener: Arc<RecordingUrlOpenPort>,
        stack: ArgStack,
        url: &str,
    ) -> SubActionOutcome {
        let mut cfg = SubActionConfig::new();
        cfg.insert("url".to_owned(), Variant::String(url.to_owned()));
        let publisher = NullPublisher;
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        CoreUrlOpenRunner::new(opener)
            .execute(&cfg, &ctx)
            .await
            .0
            .outcome
    }

    #[tokio::test]
    async fn accepts_http_and_https_and_forwards_exact_url() {
        for url in ["http://example.com", "https://example.com/path?q=1"] {
            let port = Arc::new(RecordingUrlOpenPort::new());
            let outcome = run(Arc::clone(&port), ArgStack::new(), url).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "{url}");
            assert_eq!(
                port.opened(),
                vec![url.to_owned()],
                "exact url forwarded: {url}"
            );
        }
    }

    #[tokio::test]
    async fn accepts_uppercase_and_whitespace_padded_browser_scheme() {
        // Why: URI schemes are case-insensitive (RFC 3986 §3.1); the gate trims and
        // lowercases before allowlisting, so HTTP:// and "  https://" still reach the
        // opener. A case-sensitive gate would wrongly reject these.
        for url in ["HTTP://example.com", "  https://example.com"] {
            let port = Arc::new(RecordingUrlOpenPort::new());
            let outcome = run(Arc::clone(&port), ArgStack::new(), url).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "{url}");
            assert_eq!(port.call_count(), 1, "{url}");
        }
    }

    #[tokio::test]
    async fn rejects_non_browser_schemes_without_calling_opener() {
        // SECURITY: the scheme gate MUST run before the OS opener. Asserting zero
        // recorded calls proves a malicious scheme never reaches `open`, so a gate
        // accidentally moved after the OS call would fail this test.
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "example.com", // scheme-less
            "ftp://host/resource",
            "JAVASCRIPT:alert(1)", // uppercase dangerous scheme must stay rejected
            "  file:///etc/passwd", // leading whitespace must not enable a bypass
        ] {
            let port = Arc::new(RecordingUrlOpenPort::new());
            let outcome = run(Arc::clone(&port), ArgStack::new(), url).await;
            assert!(
                matches!(outcome, SubActionOutcome::Failed(_)),
                "expected reject for {url}"
            );
            assert_eq!(port.call_count(), 0, "opener MUST NOT be called for {url}");
        }
    }

    #[tokio::test]
    async fn interpolates_var_before_scheme_check_for_accepted_url() {
        let stack = ArgStack::new().set(
            "link".to_owned(),
            Variant::String("https://example.com".to_owned()),
        );
        let port = Arc::new(RecordingUrlOpenPort::new());
        let outcome = run(Arc::clone(&port), stack, "%link%").await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(port.opened(), vec!["https://example.com".to_owned()]);
    }

    #[tokio::test]
    async fn gates_interpolated_dangerous_scheme_without_calling_opener() {
        // SECURITY: untrusted input spliced via %var% must still hit the scheme gate.
        let stack = ArgStack::new().set(
            "link".to_owned(),
            Variant::String("javascript:alert(1)".to_owned()),
        );
        let port = Arc::new(RecordingUrlOpenPort::new());
        let outcome = run(Arc::clone(&port), stack, "%link%").await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(port.call_count(), 0);
    }

    #[tokio::test]
    async fn maps_port_failure_to_failed_outcome() {
        let port = Arc::new(RecordingUrlOpenPort::failing(MockErr::Failed));
        let outcome = run(Arc::clone(&port), ArgStack::new(), "https://example.com").await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            port.call_count(),
            1,
            "scheme accepted, so the opener was invoked"
        );
    }
}
