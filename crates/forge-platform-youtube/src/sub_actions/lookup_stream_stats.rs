use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, UserText, Variant};

use crate::stream_stats::YoutubeStreamStats;
use crate::timestamp;
use crate::vocabulary::sub_action_ids;
use crate::vocabulary::variables::stream_stats as variables;

const KIND_ID: &str = sub_action_ids::LOOKUP_STREAM_STATS;

pub struct LookupStreamStatsRunner {
    stats: Arc<YoutubeStreamStats>,
}

impl LookupStreamStatsRunner {
    pub fn new(stats: Arc<YoutubeStreamStats>) -> Self {
        Self { stats }
    }
}

#[async_trait]
impl SubActionRunner for LookupStreamStatsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Stream Stats"
    }

    fn summary(&self) -> &str {
        "Fetches live-streaming details for the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream stats concurrent viewers start time broadcast live"
    }

    fn icon_name(&self) -> &str {
        "chart-line"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        match self.stats.fetch().await {
            Ok(details) => {
                let mut stack = ctx.arg_stack.clone().set(
                    variables::CONCURRENT_VIEWERS.to_owned(),
                    Variant::Int(details.concurrent_viewers),
                );
                if let Some(started) = timestamp::variant(details.actual_start) {
                    stack = stack.set(variables::ACTUAL_START_TIME.to_owned(), started);
                }
                if let Some(scheduled) = timestamp::variant(details.scheduled_start) {
                    stack = stack.set(variables::SCHEDULED_START_TIME.to_owned(), scheduled);
                }
                let stack = stack.set(
                    variables::LIVE_CHAT_ID.to_owned(),
                    Variant::String(details.live_chat_id),
                );
                (timer.success(), Some(stack))
            }
            Err(e) => (
                timer.failed(UserText::unlocalized(format!(
                    "stream stats lookup failed: {}",
                    crate::error_shape::redact_platform_error(&e)
                ))),
                None,
            ),
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
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::timestamp::tests::instant;
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    const TOKEN_SENTINEL: &str = "yt-stats-runner-token";
    const BODY_SENTINEL: &str = "yt-stats-response-body-sentinel";

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

    fn runner_on(server: &MockServer, broadcast: Option<&str>) -> LookupStreamStatsRunner {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let stats = YoutubeStreamStats::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
        LookupStreamStatsRunner::new(Arc::new(stats))
    }

    #[tokio::test]
    async fn execute_publishes_stream_fields_into_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "liveStreamingDetails": {
                        "concurrentViewers": "1234",
                        "actualStartTime": "2026-07-24T10:00:00Z",
                        "scheduledStartTime": "2026-07-24T09:30:00Z",
                        "activeLiveChatId": "lc-abc"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let runner = runner_on(&server, Some("bc"));
        let (telemetry, produced) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let stack = produced.expect("stats lookup must produce an arg stack");
        assert_eq!(
            stack.get("youtube.stream.concurrent_viewers"),
            Some(&Variant::Int(1234))
        );
        assert_eq!(
            stack.get("youtube.stream.actual_start_time"),
            Some(&Variant::Datetime(instant("2026-07-24T10:00:00Z")))
        );
        assert_eq!(
            stack.get("youtube.stream.scheduled_start_time"),
            Some(&Variant::Datetime(instant("2026-07-24T09:30:00Z")))
        );
        assert_eq!(
            stack.get("youtube.stream.live_chat_id"),
            Some(&Variant::String("lc-abc".to_owned()))
        );
    }

    #[tokio::test]
    async fn execute_leaves_a_not_yet_started_broadcast_start_time_unset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "liveStreamingDetails": {
                        "scheduledStartTime": "2026-07-24T09:30:00Z",
                        "activeLiveChatId": "lc-abc"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let runner = runner_on(&server, Some("bc"));
        let (telemetry, produced) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let stack = produced.expect("stats lookup must produce an arg stack");
        assert_eq!(stack.get("youtube.stream.actual_start_time"), None);
        assert_eq!(
            stack.get("youtube.stream.scheduled_start_time"),
            Some(&Variant::Datetime(instant("2026-07-24T09:30:00Z")))
        );
    }

    #[tokio::test]
    async fn execute_publishes_zero_viewers_and_empty_chat_id_when_api_omits_them() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{ "liveStreamingDetails": {} }]
            })))
            .mount(&server)
            .await;

        let runner = runner_on(&server, Some("bc"));
        let (telemetry, produced) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let stack = produced.expect("stats lookup must produce an arg stack");
        assert_eq!(
            stack.get("youtube.stream.concurrent_viewers"),
            Some(&Variant::Int(0))
        );
        assert_eq!(
            stack.get("youtube.stream.live_chat_id"),
            Some(&Variant::String(String::new()))
        );
        assert_eq!(stack.get("youtube.stream.actual_start_time"), None);
        assert_eq!(stack.get("youtube.stream.scheduled_start_time"), None);
    }

    #[tokio::test]
    async fn execute_fails_when_no_active_broadcast() {
        let server = MockServer::start().await;
        let runner = runner_on(&server, None);
        let (telemetry, produced) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn fetch_error_maps_to_failed_without_leaking_token_url_or_response_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(500).set_body_string(BODY_SENTINEL))
            .mount(&server)
            .await;

        let runner = runner_on(&server, Some("bc"));
        let (telemetry, _) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            !msg.code_or_text().contains(TOKEN_SENTINEL),
            "leaked token: {msg:?}"
        );
        assert!(
            !msg.code_or_text().contains(&server.uri()),
            "leaked url: {msg:?}"
        );
        assert!(
            !msg.code_or_text().contains(BODY_SENTINEL),
            "leaked response body: {msg:?}"
        );
        assert!(
            msg.code_or_text().contains("500"),
            "the status must survive so the failure stays diagnosable: {msg:?}"
        );
    }
}
