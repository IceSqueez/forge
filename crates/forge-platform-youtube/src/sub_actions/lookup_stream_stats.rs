use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_stats::YoutubeStreamStats;

const KIND_ID: &str = "youtube.lookup.stream_stats";

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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.stats.fetch().await {
            Ok(Variant::Object(map)) => {
                let mut stack = ctx.arg_stack.clone();
                for (field, key) in [
                    ("concurrent_viewers", "youtube.stream.concurrent_viewers"),
                    ("actual_start_time", "youtube.stream.actual_start_time"),
                    (
                        "scheduled_start_time",
                        "youtube.stream.scheduled_start_time",
                    ),
                    ("live_chat_id", "youtube.stream.live_chat_id"),
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
                        "stream stats returned an unexpected shape".to_owned(),
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
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-stats-runner-token";

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
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let stats =
            YoutubeStreamStats::new(token_source(), handle, quota).with_api_base(server.uri());
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
            Some(&Variant::String("2026-07-24T10:00:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("youtube.stream.scheduled_start_time"),
            Some(&Variant::String("2026-07-24T09:30:00Z".to_owned()))
        );
        assert_eq!(
            stack.get("youtube.stream.live_chat_id"),
            Some(&Variant::String("lc-abc".to_owned()))
        );
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
    async fn fetch_error_maps_to_failed_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let runner = runner_on(&server, Some("bc"));
        let (telemetry, _) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(!msg.contains(TOKEN_SENTINEL), "leaked token: {msg}");
        assert!(!msg.contains(&server.uri()), "leaked url: {msg}");
    }
}
