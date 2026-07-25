use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::channel::KickChannel;

const KIND_ID: &str = "kick.lookup.user";

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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("slug".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "slug",
            label: "Channel Slug",
            placeholder: "channel-slug",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("slug") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'slug' must be a non-empty string"
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

        let template = config.str("slug").unwrap_or_default();
        let slug = ctx.arg_stack.interpolate(template);

        if slug.is_empty() {
            return (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(
                        "slug is empty after interpolation".to_owned(),
                    ),
                    index: ctx.index,
                },
                None,
            );
        }

        let result = match (self.token_source)().await {
            Err(e) => Err(format!("token error: {e}")),
            Ok(token) => match self.client.get_channel_by_slug(&token, &slug).await {
                Ok(snapshot) => {
                    let mut stack = ctx.arg_stack.clone();
                    stack = stack.set(
                        "kick.viewer.id".to_owned(),
                        Variant::Int(snapshot.broadcaster_user_id as i64),
                    );
                    stack = stack.set(
                        "kick.viewer.username".to_owned(),
                        Variant::String(snapshot.slug),
                    );
                    stack = stack.set(
                        "kick.viewer.is_live".to_owned(),
                        Variant::Bool(snapshot.is_live),
                    );
                    stack = stack.set(
                        "kick.viewer.stream_title".to_owned(),
                        Variant::String(snapshot.stream_title),
                    );
                    stack = stack.set(
                        "kick.viewer.category_name".to_owned(),
                        Variant::String(snapshot.category_name),
                    );
                    stack = stack.set(
                        "kick.viewer.viewer_count".to_owned(),
                        Variant::Int(snapshot.viewer_count as i64),
                    );
                    Ok(stack)
                }
                Err(e) => Err(e.to_string()),
            },
        };

        match result {
            Ok(stack) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Success,
                    index: ctx.index,
                },
                Some(stack),
            ),
            Err(msg) => (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(msg),
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
    use super::*;
    use crate::sub_actions::test_support::{GrantLimiter, make_ctx, token_source};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> LookupUserRunner {
        let channel = KickChannel::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
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
            Arc::new(KickChannel::new(Arc::new(GrantLimiter))),
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
