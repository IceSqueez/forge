use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::stream_metadata::YoutubeStreamMetadata;

const KIND_ID: &str = "youtube.stream.update_privacy";

const PRIVACY_OPTIONS: &[&str] = &["public", "unlisted", "private"];
const DEFAULT_PRIVACY: &str = "public";

pub struct UpdatePrivacyRunner {
    metadata: Arc<YoutubeStreamMetadata>,
}

impl UpdatePrivacyRunner {
    pub fn new(metadata: Arc<YoutubeStreamMetadata>) -> Self {
        Self { metadata }
    }
}

#[async_trait]
impl SubActionRunner for UpdatePrivacyRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Update Stream Privacy"
    }

    fn summary(&self) -> &str {
        "Sets the privacy status of the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube stream broadcast privacy public unlisted private visibility metadata"
    }

    fn icon_name(&self) -> &str {
        "eye"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "privacy_status".to_owned(),
            Variant::String(DEFAULT_PRIVACY.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Select {
            key: "privacy_status",
            label: "Privacy",
            options: PRIVACY_OPTIONS,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("privacy_status") {
            Some(Variant::String(s)) if PRIVACY_OPTIONS.contains(&s.as_str()) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'privacy_status' must be one of public, unlisted, private"
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

        let template = config
            .get("privacy_status")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let privacy_status = ctx.arg_stack.interpolate(template);

        let outcome = if privacy_status.is_empty() {
            SubActionOutcome::Failed("privacy_status is empty after interpolation".to_owned())
        } else {
            match self.metadata.set_privacy(&privacy_status).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            }
        };

        (
            SubActionTelemetry {
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
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UpdatePrivacyRunner {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("vid-1".to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let meta =
            YoutubeStreamMetadata::new(token_source(), handle, quota).with_api_base(server.uri());
        UpdatePrivacyRunner::new(Arc::new(meta))
    }

    fn config(value: &str) -> SubActionConfig {
        BTreeMap::from([(
            "privacy_status".to_owned(),
            Variant::String(value.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_interpolates_privacy_template_then_succeeds() {
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
        let stack = ArgStack::new().set("vis".to_owned(), Variant::String("unlisted".to_owned()));

        let (telemetry, _) = runner.execute(&config("%vis%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let reqs = server.received_requests().await.unwrap();
        let put = reqs.iter().find(|r| r.method.as_str() == "PUT").unwrap();
        let body: serde_json::Value = serde_json::from_slice(&put.body).unwrap();
        assert_eq!(body["status"]["privacyStatus"], "unlisted");
    }

    #[tokio::test]
    async fn empty_privacy_after_interpolation_fails_without_http() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("x".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner.execute(&config("%x%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty privacy status must not reach the transport"
        );
    }

    #[test]
    fn validate_config_accepts_each_enum_value_and_rejects_off_enum_empty_missing_non_string() {
        let server_uri = "http://127.0.0.1:0".to_owned();
        let handle = ActiveBroadcastIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let meta =
            YoutubeStreamMetadata::new(token_source(), handle, quota).with_api_base(server_uri);
        let runner = UpdatePrivacyRunner::new(Arc::new(meta));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("public", config("public"), true),
            ("unlisted", config("unlisted"), true),
            ("private", config("private"), true),
            ("off-enum value", config("hidden"), false),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("privacy_status".to_owned(), Variant::Int(1))]),
                false,
            ),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
        }
    }
}
