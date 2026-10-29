use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::os_ports::UrlOpenPort;

const URL: TextField = TextField {
    key: "url",
    label: "URL",
    editor: TextEditor::Line {
        placeholder: "https://example.com",
    },
    default: "",
    required: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&URL]);

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let url = match URL.read(config, ctx.arg_stack) {
            Ok(url) => url,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = if is_browser_scheme(&url) {
            let opener = Arc::clone(&self.opener);
            match tokio::task::spawn_blocking(move || opener.open(url)).await {
                Ok(Ok(())) => SubActionOutcome::Success,
                Ok(Err(e)) => SubActionOutcome::Failed(e.to_string()),
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        } else {
            SubActionOutcome::Failed("core.url.open: rejected a non-http(s) URL".to_owned())
        };

        (timer.finish(outcome), None)
    }
}

fn is_browser_scheme(url: &str) -> bool {
    match url.trim().split_once(':') {
        Some((scheme, _)) => matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https"),
        None => false,
    }
}

#[cfg(test)]
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
        for url in ["HTTP://example.com", "  https://example.com"] {
            let port = Arc::new(RecordingUrlOpenPort::new());
            let outcome = run(Arc::clone(&port), ArgStack::new(), url).await;
            assert!(matches!(outcome, SubActionOutcome::Success), "{url}");
            assert_eq!(port.call_count(), 1, "{url}");
        }
    }

    #[tokio::test]
    async fn rejects_non_browser_schemes_without_calling_opener() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html;base64,PHNjcmlwdD4=",
            "example.com", // scheme-less
            "ftp://host/resource",
            "JAVASCRIPT:alert(1)", // uppercase dangerous scheme must stay rejected
            "  file:///etc/passwd", // leading whitespace must not enable a bypass
            "ftp://host/r?access_token=QUERY-SECRET", // a rejected URL may still carry a secret
        ] {
            let port = Arc::new(RecordingUrlOpenPort::new());
            let outcome = run(Arc::clone(&port), ArgStack::new(), url).await;

            let SubActionOutcome::Failed(msg) = outcome else {
                panic!("expected reject for {url}, got {outcome:?}");
            };
            assert!(
                !msg.contains(url.trim()),
                "the rejection echoed {url}: {msg}"
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
