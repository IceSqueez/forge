use std::sync::Arc;

use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::PlatformError;
use forge_storage::{BanLedgerKey, BanLedgerRepo};
use time::{Duration, OffsetDateTime};

use crate::ban_ledger::{ban_ledger_key, issued_ban, timeout_term_minutes};
use crate::gateway::{RefusalReasons, authorized, json_body, platform_error};

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
