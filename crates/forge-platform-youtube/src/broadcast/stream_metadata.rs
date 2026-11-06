use forge_net::GatewayRequest;
use forge_platform_core::PlatformError;
use forge_types::Secret;

use crate::api::data_api::{YouTubeDataApi, json_body, malformed};
use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;

pub(crate) const FETCH_COST: u32 = 1;
const UPDATE_COST: u32 = 50;
pub(crate) const VIDEOS_PATH: &str = "/videos";
pub(crate) const MISSING_SCOPE: &str = "stream metadata scope missing";

use forge_platform_core::TokenSource;

enum Field {
    Title,
    Description,
    Category,
    Privacy,
}

pub struct YouTubeStreamMetadata {
    api: YouTubeDataApi,
    access_token_source: TokenSource,
    active_broadcast_id: ActiveBroadcastIdHandle,
}

impl YouTubeStreamMetadata {
    pub fn new(
        api: YouTubeDataApi,
        access_token_source: TokenSource,
        active_broadcast_id: ActiveBroadcastIdHandle,
    ) -> Self {
        Self {
            api,
            access_token_source,
            active_broadcast_id,
        }
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

    async fn update(&self, field: Field, value: &str) -> Result<(), PlatformError> {
        let broadcast_id = self
            .active_broadcast_id
            .resolve_target()
            .await?
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "stream metadata - no active or upcoming YouTube broadcast".to_owned(),
            })?;

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::get(VIDEOS_PATH)
            .label(VIDEOS_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet,status")
            .query("id", broadcast_id.as_str());
        let body: serde_json::Value = self
            .api
            .send(FETCH_COST, request)
            .await
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))?
            .json()
            .map_err(malformed)?;

        let item = body
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "stream metadata - target broadcast video not found".to_owned(),
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
        let update_body = json_body(&merged)?;

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::put(VIDEOS_PATH)
            .label(VIDEOS_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet,status")
            .body(update_body);
        self.api
            .send(UPDATE_COST, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use forge_platform_core::PlatformError;
    use futures::future::BoxFuture;
    use serde_json::{Value, json};
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::api::quota_state::{BROADCAST_COST, SharedQuota};
    use crate::broadcast::broadcast_lookup::BroadcastLookup;
    use crate::endpoints_test_support::{data_api_charging, used_today};

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn metadata_on(server: &MockServer) -> (YouTubeStreamMetadata, SharedQuota) {
        let handle = ActiveBroadcastIdHandle::new();
        handle.set(Some("vid-1".to_owned()));
        let quota = SharedQuota::default();
        let meta = YouTubeStreamMetadata::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            handle,
        );
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(current_resource()),
            )
            .mount(server)
            .await;
    }

    async fn mount_update_ok(server: &MockServer) {
        Mock::given(method("PUT"))
            .and(path("/videos"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK)
                    .set_body_json(json!({"kind": "youtube#video"})),
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

        assert_eq!(used_today(&quota).await, FETCH_COST + UPDATE_COST);
    }

    #[tokio::test]
    async fn no_active_broadcast_returns_unsupported_without_any_http_call() {
        let server = MockServer::start().await;
        let handle = ActiveBroadcastIdHandle::new();
        let quota = SharedQuota::default();
        let meta = YouTubeStreamMetadata::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            handle,
        );

        let err = meta.set_title("x").await.unwrap_err();

        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported when no active broadcast, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no active broadcast must short-circuit before the GET"
        );
        assert_eq!(used_today(&quota).await, 0);
    }

    async fn mount_broadcasts(server: &MockServer, status: &str, reply: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .and(query_param("broadcastStatus", status))
            .respond_with(reply)
            .mount(server)
            .await;
    }

    fn listing(items: Value) -> ResponseTemplate {
        ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({ "items": items }))
    }

    fn out_of_order_upcoming() -> ResponseTemplate {
        listing(json!([
            { "id": "upcoming-late", "snippet": { "scheduledStartTime": "2026-11-20T18:00:00Z" } },
            { "id": "upcoming-soonest", "snippet": { "scheduledStartTime": "2026-11-08T18:00:00Z" } },
            { "id": "upcoming-middle", "snippet": { "scheduledStartTime": "2026-11-12T18:00:00Z" } }
        ]))
    }

    fn metadata_falling_back_on(server: &MockServer) -> (YouTubeStreamMetadata, SharedQuota) {
        let quota = SharedQuota::default();
        let handle = ActiveBroadcastIdHandle::new();
        handle.fall_back_to(BroadcastLookup::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
        ));
        let meta = YouTubeStreamMetadata::new(
            data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            handle,
        );
        (meta, quota)
    }

    async fn video_lookup_ids(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.method.as_str() == "GET" && r.url.path() == "/videos")
            .filter_map(|r| {
                r.url
                    .query_pairs()
                    .find(|(key, _)| key == "id")
                    .map(|(_, id)| id.into_owned())
            })
            .collect()
    }

    async fn write_field(meta: &YouTubeStreamMetadata, field: &str) -> Result<(), PlatformError> {
        match field {
            "title" => meta.set_title("v").await,
            "description" => meta.set_description("v").await,
            "category" => meta.set_category("24").await,
            _ => meta.set_privacy("public").await,
        }
    }

    #[tokio::test]
    async fn a_live_broadcast_is_the_write_target_and_upcoming_is_never_listed() {
        let server = MockServer::start().await;
        mount_broadcasts(
            &server,
            "active",
            listing(json!([{ "id": "live-1", "snippet": { "liveChatId": "chat-1" } }])),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/liveBroadcasts"))
            .and(query_param("broadcastStatus", "upcoming"))
            .respond_with(out_of_order_upcoming())
            .expect(0)
            .mount(&server)
            .await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, _quota) = metadata_falling_back_on(&server);

        meta.set_title("NEW TITLE").await.unwrap();

        assert_eq!(put_body(&server).await["id"], "live-1");
        server.verify().await;
    }

    #[tokio::test]
    async fn with_nothing_live_every_field_write_lands_on_the_earliest_upcoming_broadcast() {
        for field in ["title", "description", "category", "privacy"] {
            let server = MockServer::start().await;
            mount_broadcasts(&server, "active", listing(json!([]))).await;
            mount_broadcasts(&server, "upcoming", out_of_order_upcoming()).await;
            mount_fetch(&server).await;
            mount_update_ok(&server).await;
            let (meta, _quota) = metadata_falling_back_on(&server);

            write_field(&meta, field).await.unwrap();

            assert_eq!(
                video_lookup_ids(&server).await,
                vec!["upcoming-soonest".to_owned()],
                "{field}"
            );
            assert_eq!(put_body(&server).await["id"], "upcoming-soonest", "{field}");
        }
    }

    #[tokio::test]
    async fn an_upcoming_target_write_charges_both_broadcast_lookups() {
        let server = MockServer::start().await;
        mount_broadcasts(&server, "active", listing(json!([]))).await;
        mount_broadcasts(&server, "upcoming", out_of_order_upcoming()).await;
        mount_fetch(&server).await;
        mount_update_ok(&server).await;
        let (meta, quota) = metadata_falling_back_on(&server);

        meta.set_title("NEW TITLE").await.unwrap();

        assert_eq!(
            used_today(&quota).await,
            2 * BROADCAST_COST + FETCH_COST + UPDATE_COST
        );
    }

    #[tokio::test]
    async fn with_nothing_live_or_upcoming_a_write_is_unsupported_and_touches_no_video() {
        let server = MockServer::start().await;
        mount_broadcasts(&server, "active", listing(json!([]))).await;
        mount_broadcasts(
            &server,
            "upcoming",
            listing(json!([{ "id": "unscheduled", "snippet": { "title": "t" } }])),
        )
        .await;
        let (meta, _quota) = metadata_falling_back_on(&server);

        let err = meta.set_title("x").await.unwrap_err();

        assert!(
            matches!(&err, PlatformError::Unsupported { feature } if feature.contains("no active or upcoming")),
            "{err:?}"
        );
        assert!(video_lookup_ids(&server).await.is_empty());
    }

    #[tokio::test]
    async fn a_failed_upcoming_lookup_surfaces_its_error_and_writes_nothing() {
        const INTERNAL: u16 = reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16();
        let server = MockServer::start().await;
        mount_broadcasts(&server, "active", listing(json!([]))).await;
        mount_broadcasts(&server, "upcoming", ResponseTemplate::new(INTERNAL)).await;
        let (meta, _quota) = metadata_falling_back_on(&server);

        let err = meta.set_title("x").await.unwrap_err();

        assert!(
            matches!(
                err,
                PlatformError::Http {
                    status: INTERNAL,
                    ..
                }
            ),
            "{err:?}"
        );
        assert!(video_lookup_ids(&server).await.is_empty());
    }
}
