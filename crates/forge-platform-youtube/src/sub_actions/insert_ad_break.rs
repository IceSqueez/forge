use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ad_break::YoutubeAdBreak;

const KIND_ID: &str = "youtube.stream.insert_ad_break";
const MIN_DURATION_SECS: i64 = 1;
const MAX_DURATION_SECS: i64 = 3_600;

pub struct InsertAdBreakRunner {
    ad_break: Arc<YoutubeAdBreak>,
}

impl InsertAdBreakRunner {
    pub fn new(ad_break: Arc<YoutubeAdBreak>) -> Self {
        Self { ad_break }
    }
}

#[async_trait]
impl SubActionRunner for InsertAdBreakRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Insert Ad Break"
    }

    fn summary(&self) -> &str {
        "Triggers a mid-roll ad cuepoint on the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube ad break cuepoint mid-roll monetization broadcast live"
    }

    fn icon_name(&self) -> &str {
        "player-skip-forward"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("duration_secs".to_owned(), Variant::Int(30))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Integer {
            key: "duration_secs",
            label: "Length (seconds)",
            min: MIN_DURATION_SECS,
            max: MAX_DURATION_SECS,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("duration_secs") {
            Some(Variant::Int(n)) if (MIN_DURATION_SECS..=MAX_DURATION_SECS).contains(n) => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'duration_secs' must be {MIN_DURATION_SECS}..={MAX_DURATION_SECS}"
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

        let duration_secs = config
            .get("duration_secs")
            .and_then(|v| match v {
                Variant::Int(n) => u32::try_from(*n).ok(),
                _ => None,
            })
            .unwrap_or(30);

        let outcome =
            SubActionOutcome::from_result(&self.ad_break.insert_cuepoint(duration_secs).await);

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
        Arc::new(|| Box::pin(async { Ok("ad-token".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> InsertAdBreakRunner {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("bc".to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let ad_break =
            YoutubeAdBreak::new(token_source(), handle, quota).with_api_base(server.uri());
        InsertAdBreakRunner::new(Arc::new(ad_break))
    }

    fn config(duration: i64) -> SubActionConfig {
        BTreeMap::from([("duration_secs".to_owned(), Variant::Int(duration))])
    }

    #[test]
    fn validate_config_enforces_duration_bounds() {
        let handle = ActiveBroadcastIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let ad_break = YoutubeAdBreak::new(token_source(), handle, quota)
            .with_api_base("http://127.0.0.1:0".to_owned());
        let runner = InsertAdBreakRunner::new(Arc::new(ad_break));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("min boundary", config(MIN_DURATION_SECS), true),
            ("max boundary", config(MAX_DURATION_SECS), true),
            ("below min", config(MIN_DURATION_SECS - 1), false),
            ("above max", config(MAX_DURATION_SECS + 1), false),
            ("missing key", BTreeMap::new(), false),
            (
                "non-int",
                BTreeMap::from([("duration_secs".to_owned(), Variant::String("30".to_owned()))]),
                false,
            ),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
        }
    }

    #[tokio::test]
    async fn execute_forwards_configured_duration_to_transport() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config(90), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["durationSecs"], 90);
    }

    #[tokio::test]
    async fn execute_defaults_to_thirty_seconds_when_duration_absent() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveBroadcasts/cuepoint"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&BTreeMap::new(), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["durationSecs"], 30);
    }
}
