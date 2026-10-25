use std::sync::Arc;

use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;
use forge_storage::{BanLedgerKey, BanLedgerRepo};
use time::{Duration, OffsetDateTime};

use crate::api_gateway::{RefusalReasons, authorized, json_body, platform_error};
use crate::ban_ledger::{ban_ledger_key, issued_ban, timeout_term_minutes};

const BANS_PATH: &str = "/moderation/bans";

const MODERATION_REFUSALS: RefusalReasons = RefusalReasons {
    unauthorized: "moderation token rejected (401)",
    forbidden: Some("moderation forbidden (403); check moderation:ban scope"),
};

pub struct KickModeration {
    api: Gateway,
    ban_ledger: Arc<dyn BanLedgerRepo>,
}

impl KickModeration {
    pub fn new(api: Gateway, ban_ledger: Arc<dyn BanLedgerRepo>) -> Self {
        Self { api, ban_ledger }
    }

    pub async fn ban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        display_name: Option<&str>,
        reason: Option<&str>,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.post_ban(target_user_id, broadcaster_user_id, None, reason, token)
            .await?;
        self.record_issued_ban(
            target_user_id,
            broadcaster_user_id,
            display_name,
            None,
            reason,
        )
        .await;
        Ok(())
    }

    pub async fn timeout(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        duration_minutes: u32,
        display_name: Option<&str>,
        reason: Option<&str>,
        token: &str,
    ) -> Result<(), PlatformError> {
        self.post_ban(
            target_user_id,
            broadcaster_user_id,
            Some(duration_minutes),
            reason,
            token,
        )
        .await?;
        self.record_issued_ban(
            target_user_id,
            broadcaster_user_id,
            display_name,
            Some(timeout_term_minutes(duration_minutes)),
            reason,
        )
        .await;
        Ok(())
    }

    pub async fn unban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        token: &str,
    ) -> Result<(), PlatformError> {
        let body = serde_json::json!({
            "broadcaster_user_id": broadcaster_user_id,
            "user_id": target_user_id,
        });
        self.send_bans(GatewayRequest::delete(BANS_PATH), token, &body)
            .await?;
        self.forget_ban(&ban_ledger_key(
            broadcaster_user_id,
            &target_user_id.to_string(),
        ))
        .await;
        Ok(())
    }

    async fn post_ban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        duration_minutes: Option<u32>,
        reason: Option<&str>,
        token: &str,
    ) -> Result<(), PlatformError> {
        let mut body = serde_json::json!({
            "broadcaster_user_id": broadcaster_user_id,
            "user_id": target_user_id,
        });
        if let Some(minutes) = duration_minutes {
            body["duration"] = serde_json::Value::from(minutes);
        }
        if let Some(reason) = reason {
            body["reason"] = serde_json::Value::from(reason);
        }

        self.send_bans(GatewayRequest::post(BANS_PATH), token, &body)
            .await
    }

    async fn send_bans(
        &self,
        request: GatewayRequest,
        token: &str,
        body: &serde_json::Value,
    ) -> Result<(), PlatformError> {
        let request = authorized(request, token, BANS_PATH).body(json_body(body)?);
        self.api
            .send(request)
            .await
            .map(drop)
            .map_err(|failure| platform_error(failure, MODERATION_REFUSALS))
    }

    async fn record_issued_ban(
        &self,
        target_user_id: u64,
        broadcaster_user_id: u64,
        display_name: Option<&str>,
        term: Option<Duration>,
        reason: Option<&str>,
    ) {
        let viewer_id = target_user_id.to_string();
        let key = ban_ledger_key(broadcaster_user_id, &viewer_id);
        let banned_at = OffsetDateTime::now_utc();
        let known_name = match self.ban_ledger.get(&key, banned_at).await {
            Ok(stored) => stored.map(|entry| entry.viewer_name),
            Err(error) => {
                tracing::warn!(%error, "kick ban ledger lookup failed");
                None
            }
        };
        let entry = issued_ban(
            key,
            display_name
                .map(str::to_owned)
                .or(known_name)
                .unwrap_or(viewer_id),
            reason.map(str::to_owned),
            banned_at,
            term,
        );
        if let Err(error) = self.ban_ledger.upsert(&entry, banned_at).await {
            tracing::warn!(%error, "issued kick ban not recorded in the ban ledger");
        }
    }

    async fn forget_ban(&self, key: &BanLedgerKey) {
        if let Err(error) = self.ban_ledger.remove(key).await {
            tracing::warn!(%error, "lifted kick ban not removed from the ban ledger");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ban_ledger_test_support::{LedgerOp, MemoryBanLedger};
    use forge_storage::{BanLedgerEntry, BanOrigin};
    use forge_types::PlatformId;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn moderation_on(server: &MockServer) -> KickModeration {
        KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        let body = reqs.last().unwrap().body.clone();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn ban_posts_to_bans_endpoint_carrying_ids_without_duration() {
        for status in [200_u16, 201, 204] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/moderation/bans"))
                .respond_with(ResponseTemplate::new(status))
                .expect(1)
                .mount(&server)
                .await;

            let result = moderation_on(&server).ban(99, 42, None, None, "tok").await;
            assert!(result.is_ok(), "status {status} must map to Ok");

            let body = last_body(&server).await;
            assert_eq!(body["broadcaster_user_id"], 42);
            assert_eq!(body["user_id"], 99);
            assert!(
                body.get("duration").is_none(),
                "permanent ban must not carry a duration key"
            );
        }
    }

    #[tokio::test]
    async fn timeout_posts_body_carrying_duration_minutes_alongside_ids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/moderation/bans"))
            .and(body_string_contains("duration"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let result = moderation_on(&server)
            .timeout(99, 42, 15, None, None, "tok")
            .await;
        assert!(result.is_ok());

        let body = last_body(&server).await;
        assert_eq!(body["broadcaster_user_id"], 42);
        assert_eq!(body["user_id"], 99);
        assert_eq!(body["duration"], 15);
    }

    #[tokio::test]
    async fn unban_deletes_bans_endpoint_and_maps_2xx_to_ok() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let result = moderation_on(&server).unban(99, 42, "tok").await;
        assert!(result.is_ok());
    }

    const TARGET: u64 = 99;
    const BROADCASTER: u64 = 42;

    fn moderation_over(server: &MockServer, ledger: &Arc<MemoryBanLedger>) -> KickModeration {
        KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            Arc::clone(ledger) as Arc<dyn BanLedgerRepo>,
        )
    }

    async fn server_answering(http_method: &str, status: u16) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method(http_method))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    fn target_key() -> BanLedgerKey {
        BanLedgerKey {
            platform: PlatformId::Kick,
            channel_id: "42".to_owned(),
            viewer_id: "99".to_owned(),
        }
    }

    fn standing_row(viewer_name: &str, origin: BanOrigin) -> BanLedgerEntry {
        BanLedgerEntry {
            key: target_key(),
            viewer_name: viewer_name.to_owned(),
            reason: None,
            moderator: Some("ModAlice".to_owned()),
            banned_at: OffsetDateTime::now_utc() - time::Duration::minutes(1),
            expires_at: None,
            platform_ban_id: None,
            origin,
        }
    }

    async fn issue(
        moderation: &KickModeration,
        duration_minutes: Option<u32>,
        reason: Option<&str>,
    ) -> Result<(), PlatformError> {
        match duration_minutes {
            None => {
                moderation
                    .ban(TARGET, BROADCASTER, None, reason, "tok")
                    .await
            }
            Some(minutes) => {
                moderation
                    .timeout(TARGET, BROADCASTER, minutes, None, reason, "tok")
                    .await
            }
        }
    }

    #[tokio::test]
    async fn successful_ban_records_a_forge_row_under_broadcaster_and_target() {
        let server = server_answering("POST", 200).await;
        let ledger = Arc::new(MemoryBanLedger::default());
        let before = OffsetDateTime::now_utc();

        issue(&moderation_over(&server, &ledger), None, Some("spam"))
            .await
            .unwrap();

        let stored = ledger.stored(&target_key()).unwrap();
        assert!(stored.banned_at >= before);
        assert_eq!(
            stored,
            BanLedgerEntry {
                key: target_key(),
                viewer_name: "99".to_owned(),
                reason: Some("spam".to_owned()),
                moderator: None,
                banned_at: stored.banned_at,
                expires_at: None,
                platform_ban_id: None,
                origin: BanOrigin::IssuedByForge,
            }
        );
    }

    #[tokio::test]
    async fn issued_ban_term_matches_the_posted_duration_in_minutes() {
        for (duration_minutes, expected_term) in [
            (None, None),
            (Some(1), Some(time::Duration::seconds(60))),
            (Some(15), Some(time::Duration::seconds(900))),
            (Some(10080), Some(time::Duration::seconds(604_800))),
        ] {
            let server = server_answering("POST", 200).await;
            let ledger = Arc::new(MemoryBanLedger::default());

            issue(&moderation_over(&server, &ledger), duration_minutes, None)
                .await
                .unwrap();

            let stored = ledger.stored(&target_key()).unwrap();
            assert_eq!(
                stored
                    .expires_at
                    .map(|expires_at| expires_at - stored.banned_at),
                expected_term,
                "duration {duration_minutes:?}"
            );
        }
    }

    #[tokio::test]
    async fn issued_ban_names_the_viewer_by_display_name_then_stored_name_then_id() {
        for duration_minutes in [None, Some(10)] {
            for (stored_name, display_name, expected) in [
                (None, Some("Alice"), "Alice"),
                (None, None, "99"),
                (Some("Troll"), None, "Troll"),
                (Some("Troll"), Some("Alice"), "Alice"),
            ] {
                let server = server_answering("POST", 200).await;
                let ledger = Arc::new(MemoryBanLedger::default());
                if let Some(name) = stored_name {
                    ledger.seed(standing_row(name, BanOrigin::Observed));
                }
                let moderation = moderation_over(&server, &ledger);

                match duration_minutes {
                    None => {
                        moderation
                            .ban(TARGET, BROADCASTER, display_name, None, "tok")
                            .await
                    }
                    Some(minutes) => {
                        moderation
                            .timeout(TARGET, BROADCASTER, minutes, display_name, None, "tok")
                            .await
                    }
                }
                .unwrap();

                let stored = ledger.stored(&target_key()).unwrap();
                assert_eq!(
                    (stored.viewer_name.as_str(), stored.origin),
                    (expected, BanOrigin::IssuedByForge),
                    "duration {duration_minutes:?}, stored {stored_name:?}, display {display_name:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn rejected_ban_or_timeout_records_nothing() {
        for (status, duration_minutes) in [
            (400_u16, None),
            (401, None),
            (403, Some(10)),
            (429, None),
            (500, Some(10)),
        ] {
            let server = server_answering("POST", status).await;
            let ledger = Arc::new(MemoryBanLedger::default());

            let result = issue(&moderation_over(&server, &ledger), duration_minutes, None).await;

            assert!(result.is_err(), "status {status}");
            assert_eq!(ledger.row_count(), 0, "status {status}");
        }
    }

    #[tokio::test]
    async fn ban_and_timeout_succeed_when_the_ledger_fails() {
        for (failing, duration_minutes) in [
            (LedgerOp::Upsert, None),
            (LedgerOp::Upsert, Some(10)),
            (LedgerOp::Get, None),
        ] {
            let server = server_answering("POST", 200).await;
            let ledger = Arc::new(MemoryBanLedger::failing_on(failing));

            let result = issue(&moderation_over(&server, &ledger), duration_minutes, None).await;

            assert!(
                result.is_ok(),
                "{failing:?} {duration_minutes:?}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn failed_name_lookup_still_records_the_ban_under_the_numeric_id() {
        let server = server_answering("POST", 200).await;
        let ledger = Arc::new(MemoryBanLedger::failing_on(LedgerOp::Get));

        issue(&moderation_over(&server, &ledger), None, None)
            .await
            .unwrap();

        assert_eq!(
            ledger.stored(&target_key()).map(|entry| entry.viewer_name),
            Some("99".to_owned())
        );
    }

    #[tokio::test]
    async fn ban_body_carries_the_reason_only_when_given() {
        for (duration_minutes, reason) in [
            (None, Some("spam")),
            (Some(10), Some("calm down")),
            (None, None),
            (Some(10), None),
        ] {
            let server = server_answering("POST", 200).await;
            let ledger = Arc::new(MemoryBanLedger::default());

            issue(&moderation_over(&server, &ledger), duration_minutes, reason)
                .await
                .unwrap();

            let body = last_body(&server).await;
            assert_eq!(
                body.get("reason").and_then(serde_json::Value::as_str),
                reason,
                "duration {duration_minutes:?}"
            );
        }
    }

    #[tokio::test]
    async fn successful_unban_removes_the_ledger_row() {
        let server = server_answering("DELETE", 204).await;
        let ledger = Arc::new(MemoryBanLedger::default());
        ledger.seed(standing_row("Troll", BanOrigin::IssuedByForge));

        moderation_over(&server, &ledger)
            .unban(TARGET, BROADCASTER, "tok")
            .await
            .unwrap();

        assert_eq!(ledger.stored(&target_key()), None);
    }

    #[tokio::test]
    async fn rejected_unban_keeps_the_ledger_row() {
        for status in [400_u16, 401, 429, 500] {
            let server = server_answering("DELETE", status).await;
            let ledger = Arc::new(MemoryBanLedger::default());
            ledger.seed(standing_row("Troll", BanOrigin::IssuedByForge));

            let result = moderation_over(&server, &ledger)
                .unban(TARGET, BROADCASTER, "tok")
                .await;

            assert!(result.is_err(), "status {status}");
            assert!(ledger.stored(&target_key()).is_some(), "status {status}");
        }
    }

    #[tokio::test]
    async fn unban_succeeds_when_the_ledger_row_cannot_be_removed() {
        let server = server_answering("DELETE", 204).await;
        let ledger = Arc::new(MemoryBanLedger::failing_on(LedgerOp::Remove));

        let result = moderation_over(&server, &ledger)
            .unban(TARGET, BROADCASTER, "tok")
            .await;

        assert!(result.is_ok(), "{result:?}");
    }
}
