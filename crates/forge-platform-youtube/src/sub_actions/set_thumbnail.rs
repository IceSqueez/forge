use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::thumbnail::YoutubeThumbnail;

const KIND_ID: &str = "youtube.stream.set_thumbnail";

pub struct SetThumbnailRunner {
    thumbnail: Arc<YoutubeThumbnail>,
}

impl SetThumbnailRunner {
    pub fn new(thumbnail: Arc<YoutubeThumbnail>) -> Self {
        Self { thumbnail }
    }
}

#[async_trait]
impl SubActionRunner for SetThumbnailRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::YouTube
    }

    fn label(&self) -> &str {
        "Set Thumbnail"
    }

    fn summary(&self) -> &str {
        "Uploads a custom thumbnail image for the active YouTube broadcast."
    }

    fn search_text(&self) -> &str {
        "youtube thumbnail image upload photo broadcast video"
    }

    fn icon_name(&self) -> &str {
        "photo"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("image_path".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "image_path",
            label: "Image Path",
            placeholder: "~/thumb.png",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("image_path") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'image_path' must be a non-empty string"
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

        let template = config.str("image_path").unwrap_or_default();
        let image_path = ctx.arg_stack.interpolate(template);

        let outcome = if image_path.is_empty() {
            SubActionOutcome::Failed("image_path is empty after interpolation".to_owned())
        } else {
            SubActionOutcome::from_result(&self.thumbnail.set(&image_path).await)
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

    const TOKEN_SENTINEL: &str = "yt-thumb-runner-token";

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    struct TempImage(std::path::PathBuf);
    impl TempImage {
        fn new(bytes: &[u8]) -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("forge_yt_thumb_runner_{}.png", std::process::id()));
            std::fs::write(&p, bytes).unwrap();
            Self(p)
        }
        fn as_str(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for TempImage {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
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

    fn runner_on(server: &MockServer, broadcast: Option<&str>) -> SetThumbnailRunner {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let thumb =
            YoutubeThumbnail::new(token_source(), handle, quota).with_api_base(server.uri());
        SetThumbnailRunner::new(Arc::new(thumb))
    }

    fn config(image_path: &str) -> SubActionConfig {
        BTreeMap::from([(
            "image_path".to_owned(),
            Variant::String(image_path.to_owned()),
        )])
    }

    #[test]
    fn validate_config_requires_non_empty_string_image_path() {
        let server_uri = "http://127.0.0.1:0".to_owned();
        let handle = ActiveBroadcastIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let thumb = YoutubeThumbnail::new(token_source(), handle, quota).with_api_base(server_uri);
        let runner = SetThumbnailRunner::new(Arc::new(thumb));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid path", config("~/thumb.png"), true),
            ("empty path", config(""), false),
            ("missing key", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("image_path".to_owned(), Variant::Int(3))]),
                false,
            ),
        ];
        for (label, cfg, ok) in cases {
            assert_eq!(runner.validate_config(&cfg).is_ok(), ok, "case: {label}");
        }
    }

    #[tokio::test]
    async fn empty_path_after_interpolation_fails_without_upload() {
        let server = MockServer::start().await;
        let runner = runner_on(&server, Some("vid"));
        let stack = ArgStack::new().set("p".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner.execute(&config("%p%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty path must not reach the upload transport"
        );
    }

    #[tokio::test]
    async fn execute_interpolates_path_and_uploads() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/thumbnails/set"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let img = TempImage::new(b"png-bytes");
        let runner = runner_on(&server, Some("vid"));
        let stack =
            ArgStack::new().set("path".to_owned(), Variant::String(img.as_str().to_owned()));

        let (telemetry, produced) = runner.execute(&config("%path%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(produced.is_none(), "thumbnail upload produces no arg stack");
    }
}
