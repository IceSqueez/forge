use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use futures::future::BoxFuture;

use crate::api::channel::KickChannel;
use crate::vocabulary::variables;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::LOOKUP_USER;

const SLUG: TextField = TextField {
    key: config_keys::SLUG,
    label: "Channel Slug",
    editor: TextEditor::Line {
        placeholder: "channel-slug",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SLUG]);

pub struct LookupUserRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl LookupUserRunner {
    pub fn new(
        client: Arc<KickChannel>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for LookupUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Lookup User"
    }

    fn summary(&self) -> &str {
        "Looks up a Kick channel by slug and reports its identity and live state."
    }

    fn search_text(&self) -> &str {
        "kick lookup user channel slug viewer identity live"
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
        let slug = match SLUG.read(config, ctx.arg_stack) {
            Ok(slug) => slug,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if slug.is_empty() {
            return (timer.failed("slug is empty after interpolation"), None);
        }

        let token = match (self.token_source)().await {
            Ok(token) => token,
            Err(e) => return (timer.failed(format!("token error: {e}")), None),
        };
        match self.client.get_channel_by_slug(&token, &slug).await {
            Ok(snapshot) => {
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(
                        variables::viewer::ID.to_owned(),
                        Variant::Int(snapshot.broadcaster_user_id as i64),
                    )
                    .set(
                        variables::viewer::USERNAME.to_owned(),
                        Variant::String(snapshot.slug),
                    )
                    .set(
                        variables::viewer::IS_LIVE.to_owned(),
                        Variant::Bool(snapshot.is_live),
                    )
                    .set(
                        variables::viewer::STREAM_TITLE.to_owned(),
                        Variant::String(snapshot.stream_title),
                    )
                    .set(
                        variables::viewer::CATEGORY_NAME.to_owned(),
                        Variant::String(snapshot.category_name),
                    )
                    .set(
                        variables::viewer::VIEWER_COUNT.to_owned(),
                        Variant::Int(snapshot.viewer_count as i64),
                    );
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.failed(e.to_string()), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{make_ctx, token_source};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> LookupUserRunner {
        let channel = KickChannel::new(crate::endpoints_test_support::api_at(&server.uri()));
        LookupUserRunner::new(Arc::new(channel), token_source())
    }

    #[tokio::test]
    async fn lookup_publishes_the_channel_identity_into_the_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "broadcaster_user_id": 7,
                    "slug": "target-streamer",
                    "stream_title": "Ranked",
                    "category": {"id": 3, "name": "Chess"},
                    "stream": {"is_live": true, "viewer_count": 88}
                }]
            })))
            .mount(&server)
            .await;

        let stack = ArgStack::new().set("who".to_owned(), Variant::String("target".to_owned()));
        let config = BTreeMap::from([(
            "slug".to_owned(),
            Variant::String("%who%-streamer".to_owned()),
        )]);

        let (telemetry, produced) = runner_on(&server).execute(&config, &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let produced = produced.expect("a successful lookup must return an arg stack");
        assert_eq!(
            produced.get("kick.viewer.id"),
            Some(&Variant::Int(7)),
            "the interpolated slug must resolve against the live channel"
        );
        assert_eq!(
            produced.get("kick.viewer.username"),
            Some(&Variant::String("target-streamer".to_owned()))
        );
        assert_eq!(
            produced.get("kick.viewer.is_live"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            produced.get("kick.viewer.viewer_count"),
            Some(&Variant::Int(88))
        );
    }

    #[tokio::test]
    async fn empty_slug_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        let stack = ArgStack::new().set("who".to_owned(), Variant::String(String::new()));
        let config = BTreeMap::from([("slug".to_owned(), Variant::String("%who%".to_owned()))]);

        let (telemetry, produced) = runner_on(&server).execute(&config, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn validate_config_requires_a_non_empty_slug() {
        let runner = LookupUserRunner::new(
            Arc::new(KickChannel::new(
                crate::endpoints_test_support::unreachable_api(),
            )),
            token_source(),
        );
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            (
                "non-empty",
                BTreeMap::from([("slug".to_owned(), Variant::String("chan".to_owned()))]),
                true,
            ),
            (
                "empty",
                BTreeMap::from([("slug".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("slug".to_owned(), Variant::Int(7))]),
                false,
            ),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn upstream_http_error_is_reported_without_an_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let config = BTreeMap::from([("slug".to_owned(), Variant::String("chan".to_owned()))]);

        let (telemetry, produced) = runner_on(&server).execute(&config, &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
    }
}
