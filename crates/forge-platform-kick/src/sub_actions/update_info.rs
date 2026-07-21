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

const KIND_ID: &str = "kick.channel.update_info";
const MAX_TAGS: usize = 10;

pub struct UpdateInfoRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateInfoRunner {
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

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[async_trait]
impl SubActionRunner for UpdateInfoRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Update Channel Info"
    }

    fn summary(&self) -> &str {
        "Updates stream title, category, or tags on the Kick channel. Requires channel:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel update title category tags stream info"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("category_id".to_owned(), Variant::String(String::new())),
            ("tags".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Stream Title",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "category_id",
                label: "Category ID",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "tags",
                label: "Tags (comma-separated, max 10)",
                placeholder: "Leave empty to keep current",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title = config.str("title").unwrap_or_default();
        let category_id = config.str("category_id").unwrap_or_default();
        let tags_raw = config.str("tags").unwrap_or_default();

        let has_title = !title.is_empty();
        let has_category = !category_id.is_empty();
        let has_tags = !tags_raw.is_empty();

        if !has_title && !has_category && !has_tags {
            return Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: at least one of 'title', 'category_id', or 'tags' must be provided"
            )));
        }

        if has_tags {
            let count = parse_tags(tags_raw).len();
            if count > MAX_TAGS {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: tags count {count} exceeds the maximum of {MAX_TAGS}"
                )));
            }
        }

        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_title = config.str("title").unwrap_or_default();
        let raw_category = config.str("category_id").unwrap_or_default();
        let raw_tags = config.str("tags").unwrap_or_default();

        let title_resolved = ctx.arg_stack.interpolate(raw_title);
        let category_resolved = ctx.arg_stack.interpolate(raw_category);
        let tags_resolved = ctx.arg_stack.interpolate(raw_tags);

        let title = if title_resolved.is_empty() {
            None
        } else {
            Some(title_resolved)
        };

        let category_id = if category_resolved.is_empty() {
            None
        } else {
            match category_resolved.parse::<u64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    let outcome = SubActionOutcome::Failed(format!(
                        "category_id '{category_resolved}' is not a valid numeric id"
                    ));
                    return (
                        SubActionTelemetry {
                            args_in: ::std::collections::BTreeMap::new(),
                            produced: ::std::collections::BTreeMap::new(),
                            kind: KIND_ID.to_owned(),
                            started_at,
                            duration_ms: start.elapsed().as_millis() as u64,
                            outcome,
                            index: ctx.index,
                        },
                        None,
                    );
                }
            }
        };

        let tags = if tags_resolved.is_empty() {
            None
        } else {
            let parsed = parse_tags(&tags_resolved);
            if parsed.len() > MAX_TAGS {
                let outcome = SubActionOutcome::Failed(format!(
                    "tags count {} exceeds the maximum of {MAX_TAGS}",
                    parsed.len()
                ));
                return (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome,
                        index: ctx.index,
                    },
                    None,
                );
            }
            Some(parsed)
        };

        if title.is_none() && category_id.is_none() && tags.is_none() {
            let outcome = SubActionOutcome::Failed(
                "all fields are empty after interpolation; nothing to update".to_owned(),
            );
            return (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self
                .client
                .update_info(&token, title, category_id, tags)
                .await
            {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::{RateLimitOutcome, RateLimiter};
    use forge_types::EventId;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UpdateInfoRunner {
        let client = KickChannel::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        UpdateInfoRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> UpdateInfoRunner {
        let client = KickChannel::new(Arc::new(GrantLimiter));
        UpdateInfoRunner::new(Arc::new(client), token_source())
    }

    fn config(title: &str, category: &str, tags: &str) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(title.to_owned())),
            (
                "category_id".to_owned(),
                Variant::String(category.to_owned()),
            ),
            ("tags".to_owned(), Variant::String(tags.to_owned())),
        ])
    }

    #[tokio::test]
    async fn execute_with_only_interpolated_title_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String("Rust".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Live %g%", "", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn non_numeric_category_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("", "not-a-number", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric category id must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn eleven_tags_at_runtime_fail_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();
        let eleven = (1..=11)
            .map(|n| format!("t{n}"))
            .collect::<Vec<_>>()
            .join(",");

        let (telemetry, _) = runner
            .execute(&config("", "", &eleven), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the runtime tag-overflow guard must short-circuit before any HTTP call"
        );
    }

    #[tokio::test]
    async fn all_fields_empty_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("x".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%x%", "%x%", "%x%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an all-empty resolved config must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_at_least_one_field_and_caps_tags_at_ten() {
        let runner = runner_offline();
        let ten = (1..=10)
            .map(|n| format!("t{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let eleven = (1..=11)
            .map(|n| format!("t{n}"))
            .collect::<Vec<_>>()
            .join(",");

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("all empty", config("", "", ""), false),
            ("only title", config("t", "", ""), true),
            ("only category", config("", "1", ""), true),
            ("only tags", config("", "", "a,b"), true),
            ("ten tags", config("", "", &ten), true),
            ("eleven tags", config("", "", &eleven), false),
        ];

        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
