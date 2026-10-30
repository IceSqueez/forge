use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use super::step_failure;
use crate::channel_lookup::YoutubeChannelLookup;
use crate::vocabulary::variables::viewer as variables;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::LOOKUP_VIEWER;

const IDENTIFIER: TextField = TextField {
    key: config_keys::IDENTIFIER,
    label: "Channel Handle or ID",
    editor: TextEditor::Line {
        placeholder: "@handle or UC...",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&IDENTIFIER]);

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let identifier = match IDENTIFIER.read(config, ctx.arg_stack) {
            Ok(identifier) => identifier,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        match self.lookup.lookup(&identifier).await {
            Ok(profile) => {
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(
                        variables::CHANNEL_ID.to_owned(),
                        Variant::String(profile.channel_id),
                    )
                    .set(variables::TITLE.to_owned(), Variant::String(profile.title))
                    .set(
                        variables::SUBSCRIBER_COUNT.to_owned(),
                        Variant::Int(profile.subscriber_count),
                    )
                    .set(
                        variables::VIEW_COUNT.to_owned(),
                        Variant::Int(profile.view_count),
                    );
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.failed(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    const TOKEN_SENTINEL: &str = "yt-viewer-runner-token";
    const BODY_SENTINEL: &str = "yt-viewer-response-body-sentinel";

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
        let lookup = YoutubeChannelLookup::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
        );
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
        let lookup = YoutubeChannelLookup::new(
            crate::endpoints_test_support::data_api_at(uri),
            token_source(),
        );
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
    async fn execute_publishes_zero_counts_when_statistics_are_hidden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "UCchan",
                    "snippet": { "title": "Creator" },
                    "statistics": { "hiddenSubscriberCount": true }
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
            stack.get("youtube.viewer.subscriber_count"),
            Some(&Variant::Int(0))
        );
        assert_eq!(
            stack.get("youtube.viewer.view_count"),
            Some(&Variant::Int(0))
        );
        assert_eq!(
            stack.get("youtube.viewer.title"),
            Some(&Variant::String("Creator".to_owned()))
        );
    }

    #[tokio::test]
    async fn lookup_error_maps_to_failed_without_leaking_token_url_or_response_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(500).set_body_string(BODY_SENTINEL))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("@creator"), &make_ctx(&ArgStack::new()))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            !serde_json::to_string(&msg)
                .unwrap()
                .contains(TOKEN_SENTINEL),
            "leaked token: {msg:?}"
        );
        assert!(
            !serde_json::to_string(&msg).unwrap().contains(&server.uri()),
            "leaked url: {msg:?}"
        );
        assert!(
            !serde_json::to_string(&msg).unwrap().contains(BODY_SENTINEL),
            "leaked response body: {msg:?}"
        );
        let forge_types::UserText::Message(message) = &msg else {
            panic!("expected a catalogued message, got {msg:?}");
        };
        assert_eq!(message.code().to_string(), "youtube.request_refused");
        assert_eq!(
            message.arg("status"),
            Some(&forge_types::MessageArg::Int(500))
        );
    }
}
