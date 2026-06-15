use std::collections::HashMap;
use std::sync::Arc;

use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::Mutex;

use crate::live_chat_id::LiveChatIdHandle;
use crate::quota_state::{QuotaState, today_pacific};

const DEFAULT_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const BAN_COST: u32 = 50;

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubeModeration {
    client: reqwest::Client,
    access_token_source: TokenSource,
    live_chat_id: LiveChatIdHandle,
    quota: Arc<Mutex<QuotaState>>,
    /// Maps a banned channel id to the ban resource id returned by `insert`, so a
    /// later unban can target the resource YouTube's API exposes no lookup for.
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
                feature: "unban — ban id unknown for this channel (ban not issued in this session)"
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

    async fn insert_ban(
        &self,
        channel_id: &str,
        duration_seconds: Option<u32>,
    ) -> Result<String, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation — no active YouTube broadcast".to_owned(),
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::quota_state::today_pacific;

    const TOKEN_SENTINEL: &str = "yt-secret-token";
    const BAN_RESOURCE_ID: &str = "ban-resource-7";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    /// Builds a moderation client pointed at a live wiremock server with an
    /// active live-chat id, so the insert/delete paths are reachable.
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
        // A DELETE handler is mounted so that, were unban to call out, the test
        // would observe the request — proving the no-request guarantee.
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
}
