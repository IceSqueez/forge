use std::collections::HashMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::live_chat_id::LiveChatIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const BAN_COST: u32 = 50;
const MODERATOR_COST: u32 = 50;
const MODERATOR_LIST_COST: u32 = 1;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeModeration {
    client: reqwest::Client,
    access_token_source: TokenSource,
    live_chat_id: LiveChatIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    /// YouTube's API exposes no lookup for the ban resource id; unban needs it recorded here.
    ban_ids: Mutex<HashMap<String, String>>,
    api_base: String,
}

impl YoutubeModeration {
    pub fn new(
        access_token_source: TokenSource,
        live_chat_id: LiveChatIdHandle,
        quota: Arc<Mutex<QuotaState>>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token_source,
            live_chat_id,
            quota,
            ban_ids: Mutex::new(HashMap::new()),
            api_base: DEFAULT_API_BASE.to_owned(),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base;
        self
    }

    pub async fn ban(&self, channel_id: &str) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, None).await?;
        self.ban_ids
            .lock()
            .await
            .insert(channel_id.to_owned(), ban_id);
        Ok(())
    }

    pub async fn timeout(
        &self,
        channel_id: &str,
        duration_seconds: u32,
    ) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, Some(duration_seconds)).await?;
        self.ban_ids
            .lock()
            .await
            .insert(channel_id.to_owned(), ban_id);
        Ok(())
    }

    pub async fn unban(&self, channel_id: &str) -> Result<(), PlatformError> {
        let ban_id = self
            .ban_ids
            .lock()
            .await
            .get(channel_id)
            .cloned()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "unban - ban id unknown for this channel (ban not issued in this session)"
                    .to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(BAN_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/bans", self.api_base);

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&token)
            .query(&[("id", ban_id.as_str())])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 204 {
            self.ban_ids.lock().await.remove(channel_id);
            return Ok(());
        }
        Err(self.map_failure(resp).await)
    }

    pub async fn add_moderator(&self, channel_id: &str) -> Result<(), PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(MODERATOR_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/moderators", self.api_base);
        let payload = serde_json::json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "moderatorDetails": { "channelId": channel_id },
            }
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet")])
            .json(&payload)
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

    pub async fn remove_moderator(&self, channel_id: &str) -> Result<(), PlatformError> {
        let moderator_id = self.resolve_moderator_id(channel_id).await?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(MODERATOR_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/moderators", self.api_base);

        let resp = self
            .client
            .delete(&url)
            .bearer_auth(&token)
            .query(&[("id", moderator_id.as_str())])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 204 {
            return Ok(());
        }
        Err(self.map_failure(resp).await)
    }

    /// Pages the moderator list for the active broadcast to find `channel_id`'s resource id.
    async fn resolve_moderator_id(&self, channel_id: &str) -> Result<String, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        let url = format!("{}/liveChat/moderators", self.api_base);
        let mut page_token: Option<String> = None;

        loop {
            {
                let today = today_pacific();
                let mut qt = self.quota.lock().await;
                qt.charge(MODERATOR_LIST_COST, today)?;
            }

            let token = (self.access_token_source)().await?;
            let mut query: Vec<(&str, String)> = vec![
                ("part", "snippet".to_owned()),
                ("liveChatId", live_chat_id.clone()),
                ("maxResults", "50".to_owned()),
            ];
            if let Some(ref pt) = page_token {
                query.push(("pageToken", pt.clone()));
            }

            let resp = self
                .client
                .get(&url)
                .bearer_auth(&token)
                .query(&query)
                .send()
                .await
                .map_err(|e| PlatformError::Network {
                    reason: e.without_url().to_string(),
                })?;

            let status = resp.status().as_u16();
            if status != 200 {
                return Err(self.map_failure(resp).await);
            }

            let body: serde_json::Value =
                resp.json().await.map_err(|e| PlatformError::Network {
                    reason: e.without_url().to_string(),
                })?;

            if let Some(items) = body.get("items").and_then(|v| v.as_array()) {
                for item in items {
                    let matches = item
                        .pointer("/snippet/moderatorDetails/channelId")
                        .and_then(|v| v.as_str())
                        == Some(channel_id);
                    if matches {
                        let id = item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_owned();
                        return Ok(id);
                    }
                }
            }

            page_token = body
                .get("nextPageToken")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            if page_token.is_none() {
                return Err(PlatformError::Unsupported {
                    feature: "remove moderator - target channel is not a moderator of this chat"
                        .to_owned(),
                });
            }
        }
    }

    async fn insert_ban(
        &self,
        channel_id: &str,
        duration_seconds: Option<u32>,
    ) -> Result<String, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        {
            let today = today_pacific();
            let mut qt = self.quota.lock().await;
            qt.charge(BAN_COST, today)?;
        }

        let token = (self.access_token_source)().await?;
        let url = format!("{}/liveChat/bans", self.api_base);

        let mut ban_details = serde_json::json!({
            "liveChatId": live_chat_id,
            "type": if duration_seconds.is_some() { "temporary" } else { "permanent" },
            "bannedUserDetails": { "channelId": channel_id },
        });
        if let Some(secs) = duration_seconds {
            ban_details["banDurationSeconds"] = serde_json::json!(secs.to_string());
        }
        let payload = serde_json::json!({ "snippet": ban_details });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .query(&[("part", "snippet")])
            .json(&payload)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = resp.status().as_u16();
        if status == 200 || status == 201 {
            let body: serde_json::Value =
                resp.json().await.map_err(|e| PlatformError::Network {
                    reason: e.without_url().to_string(),
                })?;
            let ban_id = body
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            return Ok(ban_id);
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
                    reason: "moderation scope missing".to_owned(),
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
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::quota_state::today_pacific;

    const TOKEN_SENTINEL: &str = "yt-secret-token";
    const BAN_RESOURCE_ID: &str = "ban-resource-7";
    const MODERATOR_RESOURCE_ID: &str = "mod-resource-9";
    const TARGET_CHANNEL: &str = "UC-target";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn moderation_on(server: &MockServer) -> (YoutubeModeration, Arc<Mutex<QuotaState>>) {
        let handle = LiveChatIdHandle::new();
        handle.set(Some("lc-test".to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState {
            last_reset_date: today_pacific(),
            ..QuotaState::default()
        }));
        let moderation = YoutubeModeration::new(token_source(), handle, quota.clone())
            .with_api_base(server.uri());
        (moderation, quota)
    }

    fn mount_insert_ok(server: &MockServer) -> impl std::future::Future<Output = ()> + '_ {
        Mock::given(method("POST"))
            .and(path("/liveChat/bans"))
            .and(query_param("part", "snippet"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": BAN_RESOURCE_ID})))
            .mount(server)
    }

    #[tokio::test]
    async fn ban_posts_permanent_ban_with_target_channel_id() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.ban("UC-target").await.unwrap();

        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["snippet"]["type"], "permanent");
        assert_eq!(
            body["snippet"]["bannedUserDetails"]["channelId"],
            "UC-target"
        );
        assert!(
            body["snippet"].get("banDurationSeconds").is_none(),
            "permanent ban must not carry a duration"
        );
    }

    #[tokio::test]
    async fn timeout_posts_temporary_ban_carrying_the_duration() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.timeout("UC-target", 600).await.unwrap();

        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["snippet"]["type"], "temporary");
        assert_eq!(body["snippet"]["banDurationSeconds"], "600");
    }

    #[tokio::test]
    async fn unban_deletes_the_ban_resource_recorded_by_a_prior_ban() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/bans"))
            .and(query_param("id", BAN_RESOURCE_ID))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.ban("UC-target").await.unwrap();
        moderation.unban("UC-target").await.unwrap();

        let delete = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method == wiremock::http::Method::DELETE)
            .expect("a DELETE must reach the server");
        assert!(
            delete
                .url
                .query_pairs()
                .any(|(k, v)| k == "id" && v == BAN_RESOURCE_ID),
            "unban must target the recorded ban resource id"
        );
    }

    #[tokio::test]
    async fn unban_without_a_recorded_ban_fails_unsupported_and_sends_no_request() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/bans"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        let err = moderation.unban("UC-never-banned").await.unwrap_err();

        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported, got {err:?}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "unban with no recorded ban id must not hit the transport"
        );
    }

    #[tokio::test]
    async fn forbidden_response_maps_to_error_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/bans"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden by policy"))
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        let err = moderation.ban("UC-target").await.unwrap_err();
        let msg = err.to_string();

        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "error must not leak the bearer token: {msg}"
        );
        assert!(
            !msg.contains(&server.uri()),
            "error must not leak the request URL: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("googleapis"),
            "error must not leak the API host: {msg}"
        );
    }

    #[tokio::test]
    async fn successful_ban_charges_fifty_quota_units() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        let (moderation, quota) = moderation_on(&server);

        moderation.ban("UC-target").await.unwrap();

        assert_eq!(
            quota.lock().await.used_today,
            BAN_COST,
            "a successful insert must charge the documented ban cost"
        );
    }

    #[tokio::test]
    async fn add_moderator_inserts_with_target_channel_id_in_snippet() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/moderators"))
            .and(query_param("part", "snippet"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id": MODERATOR_RESOURCE_ID})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.add_moderator(TARGET_CHANNEL).await.unwrap();

        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(
            body["snippet"]["moderatorDetails"]["channelId"],
            TARGET_CHANNEL
        );
    }

    #[tokio::test]
    async fn add_moderator_charges_fifty_quota_units() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/moderators"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id": MODERATOR_RESOURCE_ID})),
            )
            .mount(&server)
            .await;
        let (moderation, quota) = moderation_on(&server);

        moderation.add_moderator(TARGET_CHANNEL).await.unwrap();

        assert_eq!(quota.lock().await.used_today, MODERATOR_COST);
    }

    #[tokio::test]
    async fn add_moderator_forbidden_response_fails_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/moderators"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden by policy"))
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        let err = moderation.add_moderator(TARGET_CHANNEL).await.unwrap_err();
        let msg = err.to_string();

        assert!(!msg.contains(TOKEN_SENTINEL), "leaked bearer token: {msg}");
        assert!(!msg.contains(&server.uri()), "leaked request URL: {msg}");
        assert!(
            !msg.to_lowercase().contains("googleapis"),
            "leaked API host: {msg}"
        );
    }

    async fn mount_list_single_page(server: &MockServer, channel_id: &str, resource_id: &str) {
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": resource_id,
                    "snippet": { "moderatorDetails": { "channelId": channel_id } },
                }],
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn remove_moderator_deletes_the_resolved_moderator_resource_id() {
        let server = MockServer::start().await;
        mount_list_single_page(&server, TARGET_CHANNEL, MODERATOR_RESOURCE_ID).await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/moderators"))
            .and(query_param("id", MODERATOR_RESOURCE_ID))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.remove_moderator(TARGET_CHANNEL).await.unwrap();

        let delete = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method == wiremock::http::Method::DELETE)
            .expect("a DELETE must reach the server");
        assert!(
            delete
                .url
                .query_pairs()
                .any(|(k, v)| k == "id" && v == MODERATOR_RESOURCE_ID),
            "delete must target the resolved moderator resource id"
        );
    }

    #[tokio::test]
    async fn remove_moderator_follows_page_token_to_resolve_target_on_second_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "mod-other",
                    "snippet": { "moderatorDetails": { "channelId": "UC-someone-else" } },
                }],
                "nextPageToken": "PAGE2",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .and(query_param("pageToken", "PAGE2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": MODERATOR_RESOURCE_ID,
                    "snippet": { "moderatorDetails": { "channelId": TARGET_CHANNEL } },
                }],
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/moderators"))
            .and(query_param("id", MODERATOR_RESOURCE_ID))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        moderation.remove_moderator(TARGET_CHANNEL).await.unwrap();

        let delete = server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.method == wiremock::http::Method::DELETE)
            .expect("a DELETE must reach the server");
        assert!(
            delete
                .url
                .query_pairs()
                .any(|(k, v)| k == "id" && v == MODERATOR_RESOURCE_ID),
            "delete must target the id resolved on the paged-to second page"
        );
    }

    #[tokio::test]
    async fn remove_moderator_when_target_is_not_a_moderator_fails_and_sends_no_delete() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .and(query_param_is_missing("pageToken"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "mod-a",
                    "snippet": { "moderatorDetails": { "channelId": "UC-a" } },
                }],
                "nextPageToken": "PAGE2",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .and(query_param("pageToken", "PAGE2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "items": [{
                    "id": "mod-b",
                    "snippet": { "moderatorDetails": { "channelId": "UC-b" } },
                }],
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/moderators"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        let err = moderation
            .remove_moderator(TARGET_CHANNEL)
            .await
            .unwrap_err();

        assert!(
            matches!(err, PlatformError::Unsupported { .. }),
            "expected Unsupported, got {err:?}"
        );
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method != wiremock::http::Method::DELETE),
            "a non-moderator must not trigger a DELETE"
        );
    }

    #[tokio::test]
    async fn remove_moderator_charges_one_unit_per_list_page_plus_fifty_for_delete() {
        let server = MockServer::start().await;
        mount_list_single_page(&server, TARGET_CHANNEL, MODERATOR_RESOURCE_ID).await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/moderators"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let (moderation, quota) = moderation_on(&server);

        moderation.remove_moderator(TARGET_CHANNEL).await.unwrap();

        assert_eq!(
            quota.lock().await.used_today,
            MODERATOR_LIST_COST + MODERATOR_COST,
        );
    }

    #[tokio::test]
    async fn remove_moderator_list_forbidden_fails_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/liveChat/moderators"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden by policy"))
            .mount(&server)
            .await;
        let (moderation, _quota) = moderation_on(&server);

        let err = moderation
            .remove_moderator(TARGET_CHANNEL)
            .await
            .unwrap_err();
        let msg = err.to_string();

        assert!(!msg.contains(TOKEN_SENTINEL), "leaked bearer token: {msg}");
        assert!(!msg.contains(&server.uri()), "leaked request URL: {msg}");
        assert!(
            !msg.to_lowercase().contains("googleapis"),
            "leaked API host: {msg}"
        );
    }
}
