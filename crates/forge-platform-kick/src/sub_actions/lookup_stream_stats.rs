use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};
use futures::future::BoxFuture;

use crate::api::channel::KickChannel;
use crate::vocabulary::sub_action_ids;
use crate::vocabulary::variables;

const KIND_ID: &str = sub_action_ids::LOOKUP_STREAM_STATS;

pub struct LookupStreamStatsRunner {
    client: Arc<KickChannel>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl LookupStreamStatsRunner {
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
impl SubActionRunner for LookupStreamStatsRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Kick
    }

    fn label(&self) -> &str {
        "Stream Stats"
    }

    fn summary(&self) -> &str {
        "Fetches live stream details for your own Kick channel."
    }

    fn search_text(&self) -> &str {
        "kick stream stats viewer count started category live"
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
        let token = match (self.token_source)().await {
            Ok(token) => token,
            Err(e) => return (timer.failed(format!("token error: {e}")), None),
        };
        match self.client.get_channel(&token).await {
            Ok(snapshot) => {
                let mut stack = ctx
                    .arg_stack
                    .clone()
                    .set(
                        variables::stream_stats::IS_LIVE.to_owned(),
                        Variant::Bool(snapshot.is_live),
                    )
                    .set(
                        variables::stream_stats::VIEWER_COUNT.to_owned(),
                        Variant::Int(snapshot.viewer_count as i64),
                    );
                if let Some(started) = crate::timestamp::variant(snapshot.started_at) {
                    stack = stack.set(variables::stream_stats::STARTED_AT.to_owned(), started);
                }
                let stack = stack
                    .set(
                        variables::stream_stats::CATEGORY_ID.to_owned(),
                        Variant::Int(snapshot.category_id as i64),
                    )
                    .set(
                        variables::stream_stats::CATEGORY_NAME.to_owned(),
                        Variant::String(snapshot.category_name),
                    )
                    .set(
                        variables::stream_stats::STREAM_TITLE.to_owned(),
                        Variant::String(snapshot.stream_title),
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

    fn runner_on(server: &MockServer) -> LookupStreamStatsRunner {
        let channel = KickChannel::new(crate::endpoints_test_support::api_at(&server.uri()));
        LookupStreamStatsRunner::new(Arc::new(channel), token_source())
    }

    #[tokio::test]
    async fn stats_of_the_authorized_channel_land_in_the_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "stream_title": "Marathon",
                    "category": {"id": 9, "name": "Retro"},
                    "stream": {
                        "is_live": true,
                        "viewer_count": 1200,
                        "start_time": "2026-07-25T09:00:00Z"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&BTreeMap::new(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let produced = produced.expect("a successful lookup must return an arg stack");
        assert_eq!(
            produced.get("kick.stream.is_live"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            produced.get("kick.stream.viewer_count"),
            Some(&Variant::Int(1200))
        );
        assert_eq!(
            produced.get("kick.stream.started_at"),
            Some(&Variant::Datetime(crate::timestamp::tests::instant(
                "2026-07-25T09:00:00Z"
            )))
        );
        assert_eq!(
            produced.get("kick.stream.category_id"),
            Some(&Variant::Int(9))
        );
    }

    #[tokio::test]
    async fn an_offline_channel_still_reports_success_with_a_false_live_flag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "stream_title": "Offline" }]
            })))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&BTreeMap::new(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            produced.unwrap().get("kick.stream.is_live"),
            Some(&Variant::Bool(false))
        );
    }

    #[tokio::test]
    async fn a_null_blank_absent_or_unreadable_start_time_omits_only_the_started_at_variable() {
        for (stream, is_live, viewers, what) in [
            (
                serde_json::json!({ "is_live": true, "viewer_count": 5, "start_time": null }),
                true,
                5,
                "null",
            ),
            (
                serde_json::json!({ "is_live": false, "viewer_count": 0, "start_time": "" }),
                false,
                0,
                "offline blank",
            ),
            (
                serde_json::json!({ "is_live": true, "viewer_count": 5 }),
                true,
                5,
                "absent key",
            ),
            (
                serde_json::json!({ "is_live": true, "viewer_count": 5, "start_time": "soon" }),
                true,
                5,
                "garbage",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": [{
                        "stream_title": "t",
                        "category": {"id": 9, "name": "Retro"},
                        "stream": stream
                    }]
                })))
                .mount(&server)
                .await;

            let stack = ArgStack::new();
            let (telemetry, produced) = runner_on(&server)
                .execute(&BTreeMap::new(), &make_ctx(&stack))
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success, "{what}");
            let produced = produced.unwrap();
            assert_eq!(produced.get("kick.stream.started_at"), None, "{what}");
            assert_eq!(
                produced.get("kick.stream.is_live"),
                Some(&Variant::Bool(is_live)),
                "{what}"
            );
            assert_eq!(
                produced.get("kick.stream.viewer_count"),
                Some(&Variant::Int(viewers)),
                "{what}"
            );
            assert_eq!(
                produced.get("kick.stream.category_id"),
                Some(&Variant::Int(9)),
                "{what}"
            );
        }
    }

    #[tokio::test]
    async fn upstream_failure_is_reported_without_an_arg_stack() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let stack = ArgStack::new();
        let (telemetry, produced) = runner_on(&server)
            .execute(&BTreeMap::new(), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(produced.is_none());
    }
}
