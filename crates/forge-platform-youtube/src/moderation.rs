use std::sync::Arc;

use forge_net::{ExpectedRefusal, GatewayFailure, GatewayRequest, Secret};
use forge_platform_core::PlatformError;
use forge_storage::{BanLedgerEntry, BanLedgerKey, BanLedgerRepo, BanOrigin, StorageError};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::ban_ledger::{ban_ledger_key, expiry_after};
use crate::credentials::YoutubeBroadcaster;
use crate::data_api::{DataApiError, YoutubeDataApi, json_body, malformed};
use crate::live_chat_id::LiveChatIdHandle;

const BAN_COST: u32 = 50;
const MODERATOR_COST: u32 = 50;
const MODERATOR_LIST_COST: u32 = 1;
const MODERATOR_LIST_PAGE_SIZE: &str = "50";
const BANS_PATH: &str = "/liveChat/bans";
const BAN_ALREADY_LIFTED_STATUS: u16 = reqwest::StatusCode::NOT_FOUND.as_u16();
const MODERATORS_PATH: &str = "/liveChat/moderators";
const MISSING_SCOPE: &str = "moderation scope missing";
const BANNED_OUTSIDE_FORGE: &str =
    "unban - this ban was issued outside forge, lift it in YouTube Studio";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;
type BroadcasterSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<YoutubeBroadcaster, PlatformError>> + Send + Sync>;

pub struct YoutubeModeration {
    api: YoutubeDataApi,
    access_token_source: TokenSource,
    broadcaster_source: BroadcasterSource,
    live_chat_id: LiveChatIdHandle,
    ban_ledger: Arc<dyn BanLedgerRepo>,
}

impl YoutubeModeration {
    pub fn new(
        api: YoutubeDataApi,
        access_token_source: TokenSource,
        broadcaster_source: BroadcasterSource,
        live_chat_id: LiveChatIdHandle,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        Self {
            api,
            access_token_source,
            broadcaster_source,
            live_chat_id,
            ban_ledger,
        }
    }

    pub async fn ban(
        &self,
        channel_id: &str,
        display_name: Option<&str>,
    ) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, None).await?;
        self.record_issued_ban(channel_id, display_name, ban_id, None)
            .await;
        Ok(())
    }

    pub async fn timeout(
        &self,
        channel_id: &str,
        duration_seconds: u32,
        display_name: Option<&str>,
    ) -> Result<(), PlatformError> {
        let ban_id = self.insert_ban(channel_id, Some(duration_seconds)).await?;
        self.record_issued_ban(channel_id, display_name, ban_id, Some(duration_seconds))
            .await;
        Ok(())
    }

    pub async fn unban(&self, channel_id: &str) -> Result<(), PlatformError> {
        let broadcaster = (self.broadcaster_source)().await?;
        let key = ban_ledger_key(&broadcaster.channel_id, channel_id);
        let ban_id = self
            .ban_ledger
            .get(&key, OffsetDateTime::now_utc())
            .await
            .map_err(ledger_failure)?
            .and_then(|entry| entry.platform_ban_id)
            .ok_or_else(|| PlatformError::Unsupported {
                feature: BANNED_OUTSIDE_FORGE.to_owned(),
            })?;

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::delete(BANS_PATH)
            .label(BANS_PATH)
            .bearer(Secret::new(token))
            .query("id", ban_id)
            .expect_refusal(ExpectedRefusal::status(BAN_ALREADY_LIFTED_STATUS));

        match self.api.send(BAN_COST, request).await {
            Ok(_) | Err(DataApiError::Gateway(GatewayFailure::NotFound { .. })) => {
                self.forget_ban(&key).await;
                Ok(())
            }
            Err(e) => Err(e.into_platform_error(MISSING_SCOPE)),
        }
    }

    async fn record_issued_ban(
        &self,
        viewer_channel_id: &str,
        display_name: Option<&str>,
        ban_id: Option<String>,
        duration_seconds: Option<u32>,
    ) {
        let broadcaster = match (self.broadcaster_source)().await {
            Ok(broadcaster) => broadcaster,
            Err(error) => {
                tracing::warn!(
                    error = %crate::error_shape::redact_platform_error(&error),
                    "issued youtube ban not recorded in the ban ledger"
                );
                return;
            }
        };
        let key = ban_ledger_key(&broadcaster.channel_id, viewer_channel_id);
        let banned_at = OffsetDateTime::now_utc();
        let known_name = match self.ban_ledger.get(&key, banned_at).await {
            Ok(stored) => stored.map(|entry| entry.viewer_name),
            Err(error) => {
                tracing::warn!(%error, "youtube ban ledger lookup failed");
                None
            }
        };
        let entry = BanLedgerEntry {
            key,
            viewer_name: display_name
                .map(str::to_owned)
                .or(known_name)
                .unwrap_or_else(|| viewer_channel_id.to_owned()),
            reason: None,
            moderator: Some(broadcaster.channel_title).filter(|title| !title.is_empty()),
            banned_at,
            expires_at: duration_seconds
                .and_then(|seconds| expiry_after(banned_at, u64::from(seconds))),
            platform_ban_id: ban_id,
            origin: BanOrigin::IssuedByForge,
        };
        if let Err(error) = self.ban_ledger.upsert(&entry, banned_at).await {
            tracing::warn!(%error, "issued youtube ban not recorded in the ban ledger");
        }
    }

    async fn forget_ban(&self, key: &BanLedgerKey) {
        if let Err(error) = self.ban_ledger.remove(key).await {
            tracing::warn!(%error, "lifted youtube ban not removed from the ban ledger");
        }
    }

    pub async fn add_moderator(&self, channel_id: &str) -> Result<(), PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        let payload = serde_json::json!({
            "snippet": {
                "liveChatId": live_chat_id,
                "moderatorDetails": { "channelId": channel_id },
            }
        });
        let body = json_body(&payload)?;
        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::post(MODERATORS_PATH)
            .label(MODERATORS_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet")
            .body(body);

        self.api
            .send(MODERATOR_COST, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }

    pub async fn remove_moderator(&self, channel_id: &str) -> Result<(), PlatformError> {
        let moderator_id = self.resolve_moderator_id(channel_id).await?;

        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::delete(MODERATORS_PATH)
            .label(MODERATORS_PATH)
            .bearer(Secret::new(token))
            .query("id", moderator_id);

        self.api
            .send(MODERATOR_COST, request)
            .await
            .map(drop)
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))
    }

    async fn resolve_moderator_id(&self, channel_id: &str) -> Result<String, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        let mut page_token: Option<String> = None;

        loop {
            let token = (self.access_token_source)().await?;
            let mut request = GatewayRequest::get(MODERATORS_PATH)
                .label(MODERATORS_PATH)
                .bearer(Secret::new(token))
                .query("part", "snippet")
                .query("liveChatId", live_chat_id.as_str())
                .query("maxResults", MODERATOR_LIST_PAGE_SIZE);
            if let Some(ref pt) = page_token {
                request = request.query("pageToken", pt.as_str());
            }

            let body: serde_json::Value = self
                .api
                .send(MODERATOR_LIST_COST, request)
                .await
                .map_err(|e| e.into_platform_error(MISSING_SCOPE))?
                .json()
                .map_err(malformed)?;

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
    ) -> Result<Option<String>, PlatformError> {
        let live_chat_id = self
            .live_chat_id
            .get()
            .ok_or_else(|| PlatformError::Unsupported {
                feature: "moderation - no active YouTube broadcast".to_owned(),
            })?;

        let mut ban_details = serde_json::json!({
            "liveChatId": live_chat_id,
            "type": if duration_seconds.is_some() { "temporary" } else { "permanent" },
            "bannedUserDetails": { "channelId": channel_id },
        });
        if let Some(secs) = duration_seconds {
            ban_details["banDurationSeconds"] = serde_json::json!(secs.to_string());
        }
        let payload = serde_json::json!({ "snippet": ban_details });
        let body = json_body(&payload)?;
        let token = (self.access_token_source)().await?;
        let request = GatewayRequest::post(BANS_PATH)
            .label(BANS_PATH)
            .bearer(Secret::new(token))
            .query("part", "snippet")
            .body(body);

        let created: serde_json::Value = self
            .api
            .send(BAN_COST, request)
            .await
            .map_err(|e| e.into_platform_error(MISSING_SCOPE))?
            .json()
            .map_err(malformed)?;
        Ok(created
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_owned))
    }
}

fn ledger_failure(error: StorageError) -> PlatformError {
    PlatformError::Io(std::io::Error::other(error))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::ban_ledger_test_support::{
        BroadcasterSource, LedgerOp, MemoryBanLedger, TEST_BROADCASTER_CHANNEL_ID,
        TEST_BROADCASTER_TITLE, broadcaster_source, signed_out_broadcaster_source,
        switchable_broadcaster_source, test_broadcaster,
    };
    use crate::quota_state::{QuotaState, SharedQuota, today_pacific};

    const TOKEN_SENTINEL: &str = "yt-secret-token";
    const BAN_RESOURCE_ID: &str = "ban-resource-7";
    const MODERATOR_RESOURCE_ID: &str = "mod-resource-9";
    const TARGET_CHANNEL: &str = "UC-target";

    fn token_source() -> TokenSource {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn moderation_on(server: &MockServer) -> (YoutubeModeration, SharedQuota) {
        moderation_with(
            server,
            Arc::new(MemoryBanLedger::default()),
            broadcaster_source(),
        )
    }

    fn moderation_with(
        server: &MockServer,
        ledger: Arc<MemoryBanLedger>,
        broadcaster: BroadcasterSource,
    ) -> (YoutubeModeration, SharedQuota) {
        let handle = LiveChatIdHandle::new();
        handle.set(Some("lc-test".to_owned()));
        let quota = SharedQuota::new(QuotaState {
            last_reset_date: today_pacific(),
            ..QuotaState::default()
        });
        let moderation = YoutubeModeration::new(
            crate::endpoints_test_support::data_api_charging(&server.uri(), quota.clone()),
            token_source(),
            broadcaster,
            handle,
            ledger,
        );
        (moderation, quota)
    }

    fn target_key() -> BanLedgerKey {
        ban_ledger_key(TEST_BROADCASTER_CHANNEL_ID, TARGET_CHANNEL)
    }

    fn stored_ban(platform_ban_id: Option<&str>, origin: BanOrigin) -> BanLedgerEntry {
        BanLedgerEntry {
            key: target_key(),
            viewer_name: "Troll".to_owned(),
            reason: None,
            moderator: None,
            banned_at: OffsetDateTime::now_utc(),
            expires_at: None,
            platform_ban_id: platform_ban_id.map(str::to_owned),
            origin,
        }
    }

    fn seeded_ledger(entry: BanLedgerEntry) -> Arc<MemoryBanLedger> {
        let ledger = Arc::new(MemoryBanLedger::default());
        ledger.seed(entry);
        ledger
    }

    async fn mount_insert_returning(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/liveChat/bans"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_delete_status(server: &MockServer, status: u16) {
        Mock::given(method("DELETE"))
            .and(path("/liveChat/bans"))
            .respond_with(ResponseTemplate::new(status))
            .mount(server)
            .await;
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.unwrap().len()
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

        moderation.ban(TARGET_CHANNEL, None).await.unwrap();

        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(body["snippet"]["type"], "permanent");
        assert_eq!(
            body["snippet"]["bannedUserDetails"]["channelId"],
            TARGET_CHANNEL
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

        moderation.timeout(TARGET_CHANNEL, 600, None).await.unwrap();

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

        moderation.ban(TARGET_CHANNEL, None).await.unwrap();
        moderation.unban(TARGET_CHANNEL).await.unwrap();

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
    async fn successful_ban_charges_fifty_quota_units() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        let (moderation, quota) = moderation_on(&server);

        moderation.ban(TARGET_CHANNEL, None).await.unwrap();

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

    enum Issue {
        Ban,
        Timeout(u32),
    }

    async fn issue(moderation: &YoutubeModeration, issue: &Issue) -> Result<(), PlatformError> {
        match issue {
            Issue::Ban => moderation.ban(TARGET_CHANNEL, None).await,
            Issue::Timeout(seconds) => moderation.timeout(TARGET_CHANNEL, *seconds, None).await,
        }
    }

    #[tokio::test]
    async fn issued_ban_is_recorded_with_its_ban_id_term_and_broadcaster_as_moderator() {
        for (issued, insert_body, expected_ban_id, expected_term) in [
            (
                Issue::Ban,
                json!({"id": BAN_RESOURCE_ID}),
                Some(BAN_RESOURCE_ID),
                None,
            ),
            (
                Issue::Timeout(600),
                json!({"id": BAN_RESOURCE_ID}),
                Some(BAN_RESOURCE_ID),
                Some(time::Duration::seconds(600)),
            ),
            (Issue::Ban, json!({"id": ""}), None, None),
            (
                Issue::Timeout(1),
                json!({}),
                None,
                Some(time::Duration::SECOND),
            ),
        ] {
            let server = MockServer::start().await;
            mount_insert_returning(&server, insert_body).await;
            let ledger = Arc::new(MemoryBanLedger::default());
            let (moderation, _quota) =
                moderation_with(&server, Arc::clone(&ledger), broadcaster_source());

            let before = OffsetDateTime::now_utc();
            issue(&moderation, &issued).await.unwrap();
            let after = OffsetDateTime::now_utc();

            let row = ledger
                .stored(&target_key())
                .expect("the ban must be recorded");
            assert_eq!(row.origin, BanOrigin::IssuedByForge);
            assert_eq!(row.platform_ban_id.as_deref(), expected_ban_id);
            assert!(row.banned_at >= before && row.banned_at <= after);
            assert_eq!(
                row.expires_at.map(|expires_at| expires_at - row.banned_at),
                expected_term
            );
            assert_eq!(row.moderator.as_deref(), Some(TEST_BROADCASTER_TITLE));
            assert_eq!(row.viewer_name, TARGET_CHANNEL);
            assert_eq!(row.reason, None);
        }
    }

    #[tokio::test]
    async fn issued_ban_names_the_viewer_by_display_name_then_stored_name_then_channel_id() {
        for issued in [Issue::Ban, Issue::Timeout(600)] {
            for (stored, display_name, expected) in [
                (false, Some("Alice"), "Alice"),
                (false, None, TARGET_CHANNEL),
                (true, None, "Troll"),
                (true, Some("Alice"), "Alice"),
            ] {
                let server = MockServer::start().await;
                mount_insert_ok(&server).await;
                let ledger = Arc::new(MemoryBanLedger::default());
                if stored {
                    ledger.seed(stored_ban(None, BanOrigin::Observed));
                }
                let (moderation, _quota) =
                    moderation_with(&server, Arc::clone(&ledger), broadcaster_source());

                match issued {
                    Issue::Ban => moderation.ban(TARGET_CHANNEL, display_name).await,
                    Issue::Timeout(seconds) => {
                        moderation
                            .timeout(TARGET_CHANNEL, seconds, display_name)
                            .await
                    }
                }
                .unwrap();

                let row = ledger.stored(&target_key()).unwrap();
                assert_eq!(
                    (row.viewer_name.as_str(), row.origin),
                    (expected, BanOrigin::IssuedByForge),
                    "stored {stored}, display {display_name:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn issued_ban_reports_success_when_recording_it_fails() {
        for (ledger_failure, broadcaster) in [
            (Some(LedgerOp::Upsert), broadcaster_source()),
            (Some(LedgerOp::Get), broadcaster_source()),
            (None, signed_out_broadcaster_source()),
        ] {
            for issued in [Issue::Ban, Issue::Timeout(60)] {
                let server = MockServer::start().await;
                mount_insert_ok(&server).await;
                let ledger = Arc::new(
                    ledger_failure
                        .map_or_else(MemoryBanLedger::default, MemoryBanLedger::failing_on),
                );
                let (moderation, _quota) =
                    moderation_with(&server, ledger, Arc::clone(&broadcaster));

                let outcome = issue(&moderation, &issued).await;

                assert!(
                    outcome.is_ok(),
                    "ledger failure {ledger_failure:?} must not fail the ban: {outcome:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn issued_ban_is_recorded_when_the_name_lookup_fails() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        let ledger = Arc::new(MemoryBanLedger::failing_on(LedgerOp::Get));
        let (moderation, _quota) =
            moderation_with(&server, Arc::clone(&ledger), broadcaster_source());

        moderation.ban(TARGET_CHANNEL, None).await.unwrap();

        assert_eq!(
            ledger
                .stored(&target_key())
                .and_then(|row| row.platform_ban_id),
            Some(BAN_RESOURCE_ID.to_owned())
        );
    }

    #[tokio::test]
    async fn unban_without_a_stored_ban_id_is_refused_without_a_request_or_quota() {
        for seeded in [None, Some(stored_ban(None, BanOrigin::Observed))] {
            let server = MockServer::start().await;
            mount_delete_status(&server, 204).await;
            let ledger = Arc::new(MemoryBanLedger::default());
            if let Some(entry) = seeded {
                ledger.seed(entry);
            }
            let (moderation, quota) = moderation_with(&server, ledger, broadcaster_source());

            let err = moderation.unban(TARGET_CHANNEL).await.unwrap_err();

            assert!(
                matches!(&err, PlatformError::Unsupported { feature } if feature.contains("outside forge")),
                "expected the issued-outside-forge refusal, got {err:?}"
            );
            assert_eq!(request_count(&server).await, 0);
            assert_eq!(quota.lock().await.used_today, 0);
        }
    }

    #[tokio::test]
    async fn unban_lifted_or_already_gone_removes_the_ledger_row() {
        for status in [200, 204, 404] {
            let server = MockServer::start().await;
            mount_delete_status(&server, status).await;
            let ledger = seeded_ledger(stored_ban(Some(BAN_RESOURCE_ID), BanOrigin::IssuedByForge));
            let (moderation, _quota) =
                moderation_with(&server, Arc::clone(&ledger), broadcaster_source());

            let outcome = moderation.unban(TARGET_CHANNEL).await;

            assert!(outcome.is_ok(), "status {status}: {outcome:?}");
            assert!(
                ledger.stored(&target_key()).is_none(),
                "status {status} must remove the row"
            );
        }
    }

    #[tokio::test]
    async fn unban_server_error_keeps_the_ledger_row_and_fails() {
        for status in [500, 503] {
            let server = MockServer::start().await;
            mount_delete_status(&server, status).await;
            let ledger = seeded_ledger(stored_ban(Some(BAN_RESOURCE_ID), BanOrigin::IssuedByForge));
            let (moderation, _quota) =
                moderation_with(&server, Arc::clone(&ledger), broadcaster_source());

            let err = moderation.unban(TARGET_CHANNEL).await.unwrap_err();

            assert!(
                matches!(err, PlatformError::Http { status: got, .. } if got == status),
                "status {status}: {err:?}"
            );
            assert!(ledger.stored(&target_key()).is_some());
        }
    }

    #[tokio::test]
    async fn unban_reports_success_when_removing_the_row_fails() {
        let server = MockServer::start().await;
        mount_delete_status(&server, 204).await;
        let ledger = Arc::new(MemoryBanLedger::failing_on(LedgerOp::Remove));
        ledger.seed(stored_ban(Some(BAN_RESOURCE_ID), BanOrigin::IssuedByForge));
        let (moderation, _quota) = moderation_with(&server, ledger, broadcaster_source());

        let outcome = moderation.unban(TARGET_CHANNEL).await;

        assert!(outcome.is_ok(), "{outcome:?}");
    }

    #[tokio::test]
    async fn unban_ledger_lookup_failure_is_io_without_a_request_or_quota() {
        let server = MockServer::start().await;
        mount_delete_status(&server, 204).await;
        let ledger = Arc::new(MemoryBanLedger::failing_on(LedgerOp::Get));
        let (moderation, quota) = moderation_with(&server, ledger, broadcaster_source());

        let err = moderation.unban(TARGET_CHANNEL).await.unwrap_err();

        assert!(matches!(err, PlatformError::Io(_)), "{err:?}");
        assert_eq!(request_count(&server).await, 0);
        assert_eq!(quota.lock().await.used_today, 0);
    }

    #[tokio::test]
    async fn unban_signed_out_fails_reauth_without_a_request() {
        let server = MockServer::start().await;
        mount_delete_status(&server, 204).await;
        let ledger = seeded_ledger(stored_ban(Some(BAN_RESOURCE_ID), BanOrigin::IssuedByForge));
        let (moderation, _quota) =
            moderation_with(&server, ledger, signed_out_broadcaster_source());

        let err = moderation.unban(TARGET_CHANNEL).await.unwrap_err();

        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "{err:?}"
        );
        assert_eq!(request_count(&server).await, 0);
    }

    #[tokio::test]
    async fn unban_after_switching_account_never_reads_the_previous_channel_rows() {
        let server = MockServer::start().await;
        mount_insert_ok(&server).await;
        mount_delete_status(&server, 204).await;
        let account = Arc::new(std::sync::Mutex::new(test_broadcaster()));
        let ledger = Arc::new(MemoryBanLedger::default());
        let (moderation, _quota) = moderation_with(
            &server,
            Arc::clone(&ledger),
            switchable_broadcaster_source(Arc::clone(&account)),
        );
        moderation.ban(TARGET_CHANNEL, None).await.unwrap();
        *account.lock().unwrap() = YoutubeBroadcaster {
            channel_id: "UCsecond".to_owned(),
            channel_title: "Second".to_owned(),
        };

        let err = moderation.unban(TARGET_CHANNEL).await.unwrap_err();

        assert!(matches!(err, PlatformError::Unsupported { .. }), "{err:?}");
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|r| r.method != wiremock::http::Method::DELETE),
            "the first account's ban id must not be deleted from the second account"
        );
        assert!(ledger.stored(&target_key()).is_some());
    }
}
