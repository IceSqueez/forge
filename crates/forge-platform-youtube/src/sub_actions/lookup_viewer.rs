use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::channel_lookup::YoutubeChannelLookup;

const KIND_ID: &str = "youtube.lookup.viewer";

pub struct LookupViewerRunner {
    lookup: Arc<YoutubeChannelLookup>,
}

impl LookupViewerRunner {
    pub fn new(lookup: Arc<YoutubeChannelLookup>) -> Self {
        Self { lookup }
    }
}

#[async_trait]
impl SubActionRunner for LookupViewerRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Lookup Viewer"
    }

    fn summary(&self) -> &str {
        "Looks up a YouTube channel by handle or channel id."
    }

    fn search_text(&self) -> &str {
        "youtube lookup viewer channel handle subscriber stats"
    }

    fn icon_name(&self) -> &str {
        "user"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("identifier".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "identifier",
            label: "Channel Handle or ID",
            placeholder: "@handle or UC...",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("identifier") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'identifier' must be a non-empty string"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config.str("identifier").unwrap_or_default();
        let identifier = ctx.arg_stack.interpolate(template);

        if identifier.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "identifier is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        match self.lookup.lookup(&identifier).await {
            Ok(Variant::Object(map)) => {
                let mut stack = ctx.arg_stack.clone();
                for (field, key) in [
                    ("channel_id", "youtube.viewer.channel_id"),
                    ("title", "youtube.viewer.title"),
                    ("subscriber_count", "youtube.viewer.subscriber_count"),
                    ("view_count", "youtube.viewer.view_count"),
                ] {
                    if let Some(v) = map.get(field) {
                        stack = stack.set(key.to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Ok(_) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "channel lookup returned an unexpected shape".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            ),
            Err(e) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(e.to_string()),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use tokio::sync::Mutex;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-viewer-runner-token";

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> LookupViewerRunner {
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let lookup = YoutubeChannelLookup::new(token_source(), quota).with_api_base(server.uri());
        LookupViewerRunner::new(Arc::new(lookup))
    }

    fn config(identifier: &str) -> SubActionConfig {
        BTreeMap::from([(
            "identifier".to_owned(),
            Variant::String(identifier.to_owned()),
        )])
    }

    #[test]
    fn validate_config_requires_non_empty_identifier() {
        let runner = runner_on_uri("http://127.0.0.1:0");
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid", config("@creator"), true),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("identifier".to_owned(), Variant::Int(1))]),
                false,
            ),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
        }
    }

    fn runner_on_uri(uri: &str) -> LookupViewerRunner {
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let lookup = YoutubeChannelLookup::new(token_source(), quota).with_api_base(uri.to_owned());
        LookupViewerRunner::new(Arc::new(lookup))
    }

    #[tokio::test]
    async fn empty_identifier_after_interpolation_fails_without_lookup() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("who".to_owned(), Variant::String(String::new()));

        let (telemetry, produced) = runner.execute(&config("%who%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn execute_publishes_viewer_fields_into_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "UCchan",
                    "snippet": { "title": "Creator" },
                    "statistics": { "subscriberCount": "42", "viewCount": "999" }
                }]
            })))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, produced) = runner
            .execute(&config("@creator"), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let stack = produced.expect("lookup must produce an arg stack");
        assert_eq!(
            stack.get("youtube.viewer.channel_id"),
            Some(&Variant::String("UCchan".to_owned()))
        );
        assert_eq!(
            stack.get("youtube.viewer.title"),
            Some(&Variant::String("Creator".to_owned()))
        );
        assert_eq!(
            stack.get("youtube.viewer.subscriber_count"),
            Some(&Variant::Int(42))
        );
        assert_eq!(
            stack.get("youtube.viewer.view_count"),
            Some(&Variant::Int(999))
        );
    }

    #[tokio::test]
    async fn lookup_error_maps_to_failed_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("@creator"), &make_ctx(&ArgStack::new()))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(!msg.contains(TOKEN_SENTINEL), "leaked token: {msg}");
        assert!(!msg.contains(&server.uri()), "leaked url: {msg}");
    }
}
