use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DurationBounds, DurationField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ad_break::YoutubeAdBreak;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::STREAM_INSERT_AD_BREAK;
const MIN_DURATION_SECS: i64 = 1;
const MAX_DURATION_SECS: i64 = 3_600;
const DEFAULT_DURATION_SECS: i64 = 30;

const DURATION_SECS: DurationField = DurationField {
    key: config_keys::AD_DURATION_SECS,
    label: "Length",
    default: DEFAULT_DURATION_SECS,
    bounds: DurationBounds::in_seconds(MIN_DURATION_SECS, MAX_DURATION_SECS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&DURATION_SECS]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<u32, ConfigProblem> {
    u32::try_from(DURATION_SECS.read(config, args)?.as_secs()).map_err(|_| {
        ConfigProblem::field(
            config_keys::AD_DURATION_SECS,
            ConfigReason::OutOfRange(ValueBounds::between(MIN_DURATION_SECS, MAX_DURATION_SECS)),
        )
    })
}

pub(crate) struct InsertAdBreakRunner {
    ad_break: Arc<YoutubeAdBreak>,
}

impl InsertAdBreakRunner {
    pub(crate) fn new(ad_break: Arc<YoutubeAdBreak>) -> Self {
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let duration_secs = match read_config(config, ctx.arg_stack) {
            Ok(duration_secs) => duration_secs,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.ad_break.insert_cuepoint(duration_secs).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use std::sync::Arc;

    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
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
        let ad_break = YoutubeAdBreak::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
        InsertAdBreakRunner::new(Arc::new(ad_break))
    }

    fn config(duration: i64) -> SubActionConfig {
        BTreeMap::from([("duration_secs".to_owned(), Variant::Int(duration))])
    }

    #[test]
    fn validate_config_enforces_duration_bounds_and_accepts_numeric_text() {
        let handle = ActiveBroadcastIdHandle::new();
        let ad_break = YoutubeAdBreak::new(
            crate::endpoints_test_support::data_api_at("http://127.0.0.1:0"),
            token_source(),
            handle,
        );
        let runner = InsertAdBreakRunner::new(Arc::new(ad_break));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("min boundary", config(MIN_DURATION_SECS), true),
            ("max boundary", config(MAX_DURATION_SECS), true),
            ("below min", config(MIN_DURATION_SECS - 1), false),
            ("above max", config(MAX_DURATION_SECS + 1), false),
            (
                "missing key falls back to thirty seconds",
                BTreeMap::new(),
                true,
            ),
            (
                "numeric text",
                BTreeMap::from([("duration_secs".to_owned(), Variant::String("30".to_owned()))]),
                true,
            ),
            (
                "template",
                BTreeMap::from([(
                    "duration_secs".to_owned(),
                    Variant::String("%len%".to_owned()),
                )]),
                true,
            ),
            (
                "non-numeric text",
                BTreeMap::from([(
                    "duration_secs".to_owned(),
                    Variant::String("long".to_owned()),
                )]),
                false,
            ),
            (
                "bool",
                BTreeMap::from([("duration_secs".to_owned(), Variant::Bool(true))]),
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
    async fn execute_sends_the_declared_default_or_the_resolved_text_duration() {
        let stack = ArgStack::new().set("len".to_owned(), Variant::String(" 45 ".to_owned()));
        for (case, step_config, expected) in [
            ("missing key", BTreeMap::new(), 30),
            (
                "numeric text",
                BTreeMap::from([("duration_secs".to_owned(), Variant::String("60".to_owned()))]),
                60,
            ),
            (
                "template",
                BTreeMap::from([(
                    "duration_secs".to_owned(),
                    Variant::String("%len%".to_owned()),
                )]),
                45,
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/liveBroadcasts/cuepoint"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
                .mount(&server)
                .await;

            let (telemetry, _) = runner_on(&server)
                .execute(&step_config, &make_ctx(&stack))
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success, "{case}");
            let req = &server.received_requests().await.unwrap()[0];
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            assert_eq!(body["durationSecs"], expected, "{case}");
        }
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
