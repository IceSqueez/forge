use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_UPLOAD_BASE: &str = "https://www.googleapis.com/upload/youtube/v3";
const THUMBNAIL_SET_COST: u32 = 50;
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeThumbnail {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeThumbnail {
    pub fn new(
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            active_broadcast_id,
            quota,
            api_base: DEFAULT_UPLOAD_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn set(&self, image_path: &str) -> Result<(), PlatformError> {
        let video_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "thumbnail - no active YouTube broadcast".to_owned(),
                })?;

        let bytes = tokio::fs::read(image_path).await?;
        if bytes.len() as u64 > MAX_THUMBNAIL_BYTES {
            return Err(PlatformError::Unsupported {
                feature: format!("thumbnail exceeds the {MAX_THUMBNAIL_BYTES}-byte API limit"),
            });
        }
        let content_type = content_type_for(image_path);

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(THUMBNAIL_SET_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/thumbnails/set", self.api_base);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("videoId", video_id.as_str()), ("uploadType", "media")])
            .header("Content-Type", content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 201 {
            return Ok(());
        }
        Err(self.map_failure(resp).await)
    }

    async fn map_failure(&self, resp: reqwest::Response) -> PlatformError {
        let status = resp.status().as_u16();
        let retry_after_secs = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);
        let body_text = resp.text().await.unwrap_or_default();

        match status {
            429 => PlatformError::RateLimited { retry_after_secs },
            403 if body_text.contains("quotaExceeded") => PlatformError::QuotaExhausted,
            403 if body_text.contains("insufficientPermissions")
                || body_text.contains("operationNotSupported") =>
            {
                PlatformError::Auth {
                    reason: "thumbnail upload scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}

fn content_type_for(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-thumb-secret-token";

    struct TempImage(std::path::PathBuf);

    impl TempImage {
        fn new(name: &str, bytes: &[u8]) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let uniq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut p = std::env::temp_dir();
            p.push(format!(
                "forge_yt_thumb_{}_{uniq}_{name}",
                std::process::id()
            ));
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

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn thumbnail_on(
        server: &MockServer,
        broadcast: Option<&str>,
    ) -> (YoutubeThumbnail, Arc<Mutex<QuotaState>>) {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let thumb = YoutubeThumbnail::new(token_source(), handle, Arc::clone(&quota))
            .with_api_base(server.uri());
        (thumb, quota)
    }

    #[test]
    fn content_type_maps_by_extension() {
        for (input, expected) in [
            ("photo.png", "image/png"),
            ("PHOTO.PNG", "image/png"),
            ("shot.jpg", "image/jpeg"),
            ("shot.jpeg", "image/jpeg"),
            ("shot.JPG", "image/jpeg"),
            ("noext", "application/octet-stream"),
            ("anim.gif", "application/octet-stream"),
        ] {
            assert_eq!(content_type_for(input), expected, "path: {input}");
        }
    }

    #[tokio::test]
    async fn set_returns_unsupported_when_no_active_broadcast() {
        let server = MockServer::start().await;
        let (thumb, _quota) = thumbnail_on(&server, None);
        let img = TempImage::new("t.png", b"\x89PNG\r\n");

        let err = thumb.set(img.as_str()).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported without a broadcast, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no broadcast must short-circuit before any upload"
        );
    }

    #[tokio::test]
    async fn set_uploads_with_media_type_content_type_and_charges_quota() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/thumbnails/set"))
            .and(query_param("videoId", "vid-123"))
            .and(query_param("uploadType", "media"))
            .and(header("Content-Type", "image/png"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"kind": "youtube#thumbnailSetResponse"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (thumb, quota) = thumbnail_on(&server, Some("vid-123"));
        let img = TempImage::new("up.png", b"png-bytes-here");

        thumb.set(img.as_str()).await.unwrap();

        assert_eq!(
            quota.lock().await.used_today,
            THUMBNAIL_SET_COST,
            "successful upload must charge {THUMBNAIL_SET_COST} units"
        );
    }

    #[tokio::test]
    async fn set_rejects_file_one_byte_over_cap_without_uploading() {
        let server = MockServer::start().await;
        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let img = TempImage::new("big.png", &vec![0u8; (MAX_THUMBNAIL_BYTES + 1) as usize]);

        let err = thumb.set(img.as_str()).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "over-cap file must be rejected as Unsupported, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "over-cap file must not reach the upload endpoint"
        );
    }

    #[tokio::test]
    async fn set_accepts_file_exactly_at_cap() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/thumbnails/set"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let img = TempImage::new("cap.png", &vec![0u8; MAX_THUMBNAIL_BYTES as usize]);

        thumb.set(img.as_str()).await.unwrap();
    }

    #[tokio::test]
    async fn set_returns_io_error_for_missing_file_before_any_upload() {
        let server = MockServer::start().await;
        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let missing = format!(
            "{}/forge_yt_thumb_definitely_absent_{}.png",
            std::env::temp_dir().display(),
            std::process::id()
        );

        let err = thumb.set(&missing).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Io(_)),
            "missing file must surface as Io, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "unreadable file must short-circuit before upload"
        );
    }

    #[tokio::test]
    async fn set_maps_403_quota_exceeded_to_quota_exhausted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/thumbnails/set"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string(r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#),
            )
            .mount(&server)
            .await;

        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let img = TempImage::new("q.png", b"x");

        let err = thumb.set(img.as_str()).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::QuotaExhausted),
            "403 quotaExceeded must map to QuotaExhausted, got: {err}"
        );
    }

    #[tokio::test]
    async fn set_error_does_not_leak_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/thumbnails/set"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let img = TempImage::new("e.png", b"x");

        let err = thumb.set(img.as_str()).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "error must not leak the bearer token: {msg}"
        );
        assert!(
            !msg.contains(&server.uri()),
            "error must not leak the request URL: {msg}"
        );
    }
}
