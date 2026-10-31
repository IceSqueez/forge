use std::path::PathBuf;
use std::sync::Arc;

use forge_net::{GatewayRequest, RequestBody};
use forge_platform_core::PlatformError;
use forge_types::Secret;
use futures::future::BoxFuture;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::data_api::YoutubeDataApi;

const THUMBNAIL_SET_COST: u32 = 50;
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;
const THUMBNAIL_SET_PATH: &str = "/thumbnails/set";
const UNSUPPORTED_IMAGE_TYPE: &str = "thumbnail must be a .png, .jpg or .jpeg file";
const HOME_PREFIX: &str = "~/";
const MISSING_SCOPE: &str = "thumbnail upload scope missing";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeThumbnail {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
}

impl YoutubeThumbnail {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
    ) -> Self {
        Self {
            api,
            access_token_source,
            active_broadcast_id,
        }
    }

    pub async fn set(&self, image_path: &str) -> Result<(), PlatformError> {
        let video_id = self.active_broadcast_id.resolve().await?.ok_or_else(|| {
            PlatformError::Unsupported {
                feature: "thumbnail - no active YouTube broadcast".to_owned(),
            }
        })?;

        let content_type = content_type_for(image_path).ok_or_else(|| PlatformError::Refused {
            reason: UNSUPPORTED_IMAGE_TYPE.to_owned(),
        })?;
        let size_limit_error = || PlatformError::Unsupported {
            feature: format!("thumbnail exceeds the {MAX_THUMBNAIL_BYTES}-byte API limit"),
        };
        let image_path = expanded_home(image_path);
        if tokio::fs::metadata(&image_path).await?.len() > MAX_THUMBNAIL_BYTES {
            return Err(size_limit_error());
        }
        let bytes = tokio::fs::read(&image_path).await?;
        if bytes.len() as u64 > MAX_THUMBNAIL_BYTES {
            return Err(size_limit_error());
        }

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::post(THUMBNAIL_SET_PATH)
            .label(THUMBNAIL_SET_PATH)
            .bearer(Secret::new(token))
            .query("videoId", video_id)
            .query("uploadType", "media")
            .body(RequestBody::raw(content_type, bytes));

        self.api
            .upload(THUMBNAIL_SET_COST, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }
}

fn expanded_home(path: &str) -> PathBuf {
    match (path.strip_prefix(HOME_PREFIX), std::env::home_dir()) {
        (Some(relative), Some(home)) => home.join(relative),
        _ => PathBuf::from(path),
    }
}

fn content_type_for(path: &str) -> Option<&'static str> {
    let lower = path.to_lowercase();
    if lower.ends_with(".png") {
        Some("image/png")
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::endpoints_test_support::{data_api_charging, used_today};
    use crate::quota_state::SharedQuota;

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
            #[expect(clippy::let_underscore_must_use, reason = "cleanup is best effort")]
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok("yt-thumb-token".to_owned()) }))
    }

    fn thumbnail_on(
        server: &MockServer,
        broadcast: Option<&str>,
    ) -> (YoutubeThumbnail, SharedQuota) {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(broadcast.map(|s| s.to_owned()));
        let quota = SharedQuota::default();
        let thumb = YoutubeThumbnail::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            handle,
        );
        (thumb, quota)
    }

    #[test]
    fn content_type_maps_by_extension() {
        for (input, expected) in [
            ("photo.png", Some("image/png")),
            ("PHOTO.PNG", Some("image/png")),
            ("shot.jpg", Some("image/jpeg")),
            ("shot.jpeg", Some("image/jpeg")),
            ("shot.JPG", Some("image/jpeg")),
            ("noext", None),
            ("anim.gif", None),
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
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(json!({"kind": "youtube#thumbnailSetResponse"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let (thumb, quota) = thumbnail_on(&server, Some("vid-123"));
        let img = TempImage::new("up.png", b"png-bytes-here");

        thumb.set(img.as_str()).await.unwrap();

        assert_eq!(used_today(&quota).await, THUMBNAIL_SET_COST);
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({"kind": "x"})),
            )
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
    async fn set_refuses_unsupported_extension_before_reading_the_file() {
        let server = MockServer::start().await;
        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let absent = format!(
            "{}/forge_yt_thumb_absent_{}.gif",
            std::env::temp_dir().display(),
            std::process::id()
        );

        let err = thumb.set(&absent).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Refused { .. }),
            "bad extension must be refused before any file access, got: {err}"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_refuses_oversized_file_from_metadata_alone() {
        const SPARSE_LEN: u64 = 256 * 1024 * 1024;
        let server = MockServer::start().await;
        let (thumb, _quota) = thumbnail_on(&server, Some("vid"));
        let img = TempImage::new("sparse.png", b"");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&img.0)
            .unwrap()
            .set_len(SPARSE_LEN)
            .unwrap();

        let err = thumb.set(img.as_str()).await.unwrap_err();
        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "oversized file must be refused as Unsupported, got: {err}"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn only_a_leading_home_prefix_is_expanded() {
        let home = std::env::home_dir().expect("the test host has a home directory");

        assert_eq!(expanded_home("~/art/thumb.png"), home.join("art/thumb.png"));
        for literal in ["/art/thumb.png", "art/~/thumb.png", "~other/thumb.png", "~"] {
            assert_eq!(expanded_home(literal), PathBuf::from(literal), "{literal}");
        }
    }
}
