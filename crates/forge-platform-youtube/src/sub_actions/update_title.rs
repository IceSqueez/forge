use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, UserText};

use crate::stream_metadata::YoutubeStreamMetadata;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::STREAM_UPDATE_TITLE;

const TITLE: TextField = TextField {
    key: config_keys::TITLE,
    label: "Title",
    editor: TextEditor::Line {
        placeholder: "New stream title",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TITLE]);

pub struct UpdateTitleRunner {
    metadata: Arc<YoutubeStreamMetadata>,
}

impl UpdateTitleRunner {
    pub fn new(metadata: Arc<YoutubeStreamMetadata>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl SubActionRunner for UpdateTitleRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Update Stream Title"
    }

    fn summary(&self) -> &str {
        "Sets the title of the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream broadcast title rename metadata"
    }

    fn icon_name(&self) -> &str {
        "edit"
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
        let title = match TITLE.read(config, ctx.arg_stack) {
            Ok(title) => title,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if title.is_empty() {
            return (
                timer.failed(UserText::unlocalized("title is empty after interpolation")),
                None,
            );
        }

        let outcome = SubActionOutcome::from_result(&self.metadata.set_title(&title).await);
        (timer.finish(outcome), None)
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
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use forge_types::Variant;
    use std::collections::BTreeMap;

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
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UpdateTitleRunner {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("vid-1".to_owned()));
        let meta = YoutubeStreamMetadata::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
        UpdateTitleRunner::new(Arc::new(meta))
    }

    fn config(title: &str) -> SubActionConfig {
        BTreeMap::from([("title".to_owned(), Variant::String(title.to_owned()))])
    }

    #[tokio::test]
    async fn execute_interpolates_title_template_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{ "id": "vid-1", "snippet": {}, "status": {} }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("PUT"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("game".to_owned(), Variant::String("Chess".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Playing %game%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let reqs = server.received_requests().await.unwrap();
        let put = reqs.iter().find(|r| r.method.as_str() == "PUT").unwrap();
        let body: serde_json::Value = serde_json::from_slice(&put.body).unwrap();
        assert_eq!(body["snippet"]["title"], "Playing Chess");
    }

    #[tokio::test]
    async fn empty_title_after_interpolation_fails_without_http() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("x".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner.execute(&config("%x%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty title must not reach the transport"
        );
    }

    #[test]
    fn validate_config_accepts_valid_rejects_empty_missing_and_non_string() {
        let server_uri = "http://127.0.0.1:0".to_owned();
        let handle = ActiveBroadcastIdHandle::new();
        let meta = YoutubeStreamMetadata::new(
            crate::endpoints_test_support::data_api_at(&server_uri),
            token_source(),
            handle,
        );
        let runner = UpdateTitleRunner::new(Arc::new(meta));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid", config("New Title"), true),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("title".to_owned(), Variant::Int(3))]),
                false,
            ),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
        }
    }
}
