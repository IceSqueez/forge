use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const FETCH_COST: u32 = 1;
const UPDATE_COST: u32 = 50;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

enum Field {
    Title,
    Description,
    Category,
    Privacy,
}

pub struct YoutubeStreamMetadata {
    client: reqwest::Client,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    api_base: String,
}

impl YoutubeStreamMetadata {
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
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn set_title(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Title, value).await
    }

    pub async fn set_description(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Description, value).await
    }

    pub async fn set_category(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Category, value).await
    }

    pub async fn set_privacy(&self, value: &str) -> Result<(), PlatformError> {
        self.update(Field::Privacy, value).await
    }

    /// `videos.update` clears any `part` field omitted from the request body, so the
    /// current `snippet`+`status` is fetched, the target field merged in, and written back.
    async fn update(&self, field: Field, value: &str) -> Result<(), PlatformError> {
        let broadcast_id =
            self.active_broadcast_id
                .get()
                .ok_or_else(|| PlatformError::Unsupported {
                    feature: "stream metadata - no active YouTube broadcast".to_owned(),
                })?;

        let url = format!("{}/videos", self.api_base);

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(FETCH_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,status"), ("id", broadcast_id.as_str())])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status != 200 {
            return Err(self.map_failure(resp).await);
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "stream metadata - active broadcast video not found".to_owned(),
            })?;

        let mut snippet = item
            .get("snippet")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut status_part = item
            .get("status")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        match field {
            Field::Title => snippet["title"] = serde_json::json!(value),
            Field::Description => snippet["description"] = serde_json::json!(value),
            Field::Category => snippet["categoryId"] = serde_json::json!(value),
            Field::Privacy => status_part["privacyStatus"] = serde_json::json!(value),
        }

        let merged = serde_json::json!({
            "id": broadcast_id,
            "snippet": snippet,
            "status": status_part,
        });

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(UPDATE_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet,status")])
            .json(&merged)
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
                    reason: "stream metadata scope missing".to_owned(),
                }
            }
            _ => PlatformError::Http {
                status,
                body: body_text,
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use forge_platform_core::PlatformError;
    use futures::future::BoxFuture;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN_SENTINEL: &str = "test-token";

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn metadata_on(server: &MockServer) -> (YoutubeStreamMetadata, Arc<Mutex<QuotaState>>) {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("vid-1".to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let meta = YoutubeStreamMetadata::new(token_source(), handle, Arc::clone(&quota))
            .with_api_base(server.uri());
        (meta, quota)
    }

    fn current_resource() -> Value {
        json!({
            "items": [{
                "id": "vid-1",
                "snippet": {
                    "title": "OLD TITLE",
                    "description": "OLD DESC",
                    "categoryId": "20"
                },
                "status": {
                    "privacyStatus": "private"
                }
            }]
        })
    }

    async fn mount_fetch(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(current_resource()))
            .mount(server)
            .await;
    }

    async fn mount_update_ok(server: &MockServer) {
        Mock::given(method("PUT"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"kind": "youtube#video"})),
            )
            .mount(server)
            .await;
    }

    async fn put_body(server: &MockServer) -> Value {
        let reqs = server.received_requests().await.unwrap();
        let put = reqs
            .iter()
            .find(|r| r.method.as_str() == "PUT")
            .expect("a PUT must have been issued");
        serde_json::from_slice(&put.body).unwrap()
    }

    #[tokio::test]
    async fn set_title_merges_new_title_and_preserves_other_snippet_and_status_fields() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, _quota) = metadata_on(&server);

        meta.set_title("NEW TITLE").await.unwrap();

        let body = put_body(&server).await;
        assert_eq!(body["snippet"]["title"], "NEW TITLE");
        assert_eq!(body["snippet"]["description"], "OLD DESC");
        assert_eq!(body["snippet"]["categoryId"], "20");
        assert_eq!(body["status"]["privacyStatus"], "private");
    }

    #[tokio::test]
    async fn set_description_merges_into_snippet_and_preserves_other_fields() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, _quota) = metadata_on(&server);

        meta.set_description("NEW DESC").await.unwrap();

        let body = put_body(&server).await;
        assert_eq!(body["snippet"]["description"], "NEW DESC");
        assert_eq!(body["snippet"]["title"], "OLD TITLE");
        assert_eq!(body["snippet"]["categoryId"], "20");
        assert_eq!(body["status"]["privacyStatus"], "private");
    }

    #[tokio::test]
    async fn set_category_merges_category_id_into_snippet_and_preserves_other_fields() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, _quota) = metadata_on(&server);

        meta.set_category("24").await.unwrap();

        let body = put_body(&server).await;
        assert_eq!(body["snippet"]["categoryId"], "24");
        assert_eq!(body["snippet"]["title"], "OLD TITLE");
        assert_eq!(body["snippet"]["description"], "OLD DESC");
        assert_eq!(body["status"]["privacyStatus"], "private");
    }

    #[tokio::test]
    async fn set_privacy_merges_into_status_and_preserves_snippet_fields() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, _quota) = metadata_on(&server);

        meta.set_privacy("public").await.unwrap();

        let body = put_body(&server).await;
        assert_eq!(body["status"]["privacyStatus"], "public");
        assert_eq!(body["snippet"]["title"], "OLD TITLE");
        assert_eq!(body["snippet"]["description"], "OLD DESC");
        assert_eq!(body["snippet"]["categoryId"], "20");
    }

    #[tokio::test]
    async fn successful_update_charges_fetch_plus_update_quota() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, quota) = metadata_on(&server);

        meta.set_title("anything").await.unwrap();

        let qt = quota.lock().await;
        assert_eq!(
            qt.used_today,
            FETCH_COST + UPDATE_COST,
            "must charge fetch ({FETCH_COST}) + update ({UPDATE_COST}) = 51 units"
        );
    }

    #[tokio::test]
    async fn no_active_broadcast_returns_unsupported_without_any_http_call() {
        let server = MockServer::start().await;
        let handle = ActiveBroadcastIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let meta = YoutubeStreamMetadata::new(token_source(), handle, Arc::clone(&quota))
            .with_api_base(server.uri());

        let err = meta.set_title("x").await.unwrap_err();

        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported when no active broadcast, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no active broadcast must short-circuit before the GET"
        );
        let qt = quota.lock().await;
        assert_eq!(qt.used_today, 0, "no broadcast must not charge quota");
    }

    #[tokio::test]
    async fn quota_exhausted_returns_without_issuing_http() {
        let server = MockServer::start().await;
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("vid-1".to_owned()));

        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let today = today_pacific();
        {
            let mut qt = quota.lock().await;
            qt.used_today = 10_000;
            qt.peak_seen = 10_000;
            qt.last_reset_date = today;
        }
        let meta = YoutubeStreamMetadata::new(token_source(), handle, Arc::clone(&quota))
            .with_api_base(server.uri());

        let err = meta.set_title("x").await.unwrap_err();

        assert!(
            matches!(err, PlatformError::QuotaExhausted),
            "expected QuotaExhausted at cap, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "quota exhaustion must short-circuit before any HTTP call"
        );
    }

    async fn update_failure_for(status: u16, body: &str) -> PlatformError {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        Mock::given(method("PUT"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        let (meta, _quota) = metadata_on(&server);
        meta.set_title("x").await.unwrap_err()
    }

    #[tokio::test]
    async fn put_403_quota_exceeded_maps_to_quota_exhausted() {
        let err =
            update_failure_for(403, r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#).await;
        assert!(matches!(err, PlatformError::QuotaExhausted), "got: {err}");
    }

    #[tokio::test]
    async fn put_403_insufficient_permissions_maps_to_auth() {
        let err = update_failure_for(
            403,
            r#"{"error":{"errors":[{"reason":"insufficientPermissions"}]}}"#,
        )
        .await;
        assert!(matches!(err, PlatformError::Auth { .. }), "got: {err}");
    }

    #[tokio::test]
    async fn put_429_maps_to_rate_limited() {
        let err = update_failure_for(429, "too many requests").await;
        assert!(
            matches!(err, PlatformError::RateLimited { .. }),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn put_500_maps_to_http() {
        let err = update_failure_for(500, "internal error").await;
        assert!(
            matches!(err, PlatformError::Http { status: 500, .. }),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn update_error_does_not_leak_bearer_token_or_url() {
        let server = MockServer::start().await;
        mount_fetch(&server).await;
        Mock::given(method("PUT"))
            .and(path("/videos"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;
        let (meta, _quota) = metadata_on(&server);

        let err = meta.set_title("x").await.unwrap_err();
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
