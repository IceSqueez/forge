use async_trait::async_trait;
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility,
};
use forge_storage::{BAN_LEDGER_LIST_CAP, BanLedgerEntry, ViewerPlatform};
use time::OffsetDateTime;

use crate::builtin::KickIntegrationBundle;

fn ban_entry_of(entry: BanLedgerEntry) -> BanEntry {
    BanEntry {
        viewer_id: entry.key.viewer_id,
        name: entry.viewer_name,
        reason: entry.reason,
        moderator: entry.moderator,
        created_at: Some(entry.banned_at),
        duration: entry
            .expires_at
            .map_or(BanDuration::Permanent, BanDuration::Until),
        unban: UnbanAbility::Allowed,
    }
}

#[async_trait]
impl BanListSource for KickIntegrationBundle {
    async fn list_bans(&self, _after: Option<&BanPageToken>) -> BanListOutcome {
        let broadcaster_user_id = match self.credentials_manager().load().await {
            Ok(Some(credentials)) => credentials.user_id,
            Ok(None) => return BanListOutcome::Unavailable(BanListUnavailable::NotConnected),
            Err(error) => {
                tracing::warn!(%error, "kick credentials unreadable for the ban list");
                return BanListOutcome::Unavailable(BanListUnavailable::Transport);
            }
        };
        match self
            .platform()
            .ban_ledger()
            .list_active(
                ViewerPlatform::Kick,
                &broadcaster_user_id.to_string(),
                OffsetDateTime::now_utc(),
                BAN_LEDGER_LIST_CAP,
            )
            .await
        {
            Ok(entries) => BanListOutcome::Page(BanPage {
                entries: entries.into_iter().map(ban_entry_of).collect(),
                next: None,
            }),
            Err(error) => {
                tracing::warn!(%error, "kick ban ledger list failed");
                BanListOutcome::Unavailable(BanListUnavailable::Transport)
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration as StdDuration;

    use forge_platform_core::{PlatformError, RateLimitOutcome, RateLimiter, ViewerReport};
    use forge_storage::{
        BanLedgerKey, BanLedgerRepo, BanOrigin, CredentialId, CredentialsRepo, StorageError,
    };
    use time::Duration;
    use tokio::sync::watch;

    use super::*;
    use crate::ban_ledger_test_support::{LedgerOp, MemoryBanLedger};
    use crate::chat_platform::KickPlatform;
    use crate::credentials::KickCredentials;
    use crate::credentials_manager::KickCredentialsManager;
    use crate::poller::PollerAuth;

    enum StoredCredentials {
        Absent,
        Broadcaster(u64),
        Corrupt,
        Unreadable,
    }

    struct SwitchableCredentials(Mutex<StoredCredentials>);

    impl SwitchableCredentials {
        fn switch_to(&self, stored: StoredCredentials) {
            *self.0.lock().unwrap() = stored;
        }
    }

    #[async_trait]
    impl CredentialsRepo for SwitchableCredentials {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            match *self.0.lock().unwrap() {
                StoredCredentials::Absent => Ok(None),
                StoredCredentials::Broadcaster(user_id) => Ok(Some(
                    serde_json::to_string(&KickCredentials {
                        access_token: "tok".to_owned(),
                        refresh_token: "ref".to_owned(),
                        user_id,
                        username: "streamer".to_owned(),
                        client_id: "cid".to_owned(),
                        expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
                    })
                    .unwrap(),
                )),
                StoredCredentials::Corrupt => Ok(Some("{not json".to_owned())),
                StoredCredentials::Unreadable => Err(StorageError::Connection {
                    reason: "credentials store offline".to_owned(),
                }),
            }
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }
        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        async fn observe_remote_throttle(&self, _: StdDuration) {}
    }

    fn bundle_over(
        credentials: Arc<SwitchableCredentials>,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> Arc<KickIntegrationBundle> {
        let endpoints = crate::endpoints_test_support::unreachable_endpoints();
        let manager = Arc::new(KickCredentialsManager::new(
            &endpoints,
            credentials,
            "cid".to_owned(),
            "secret".to_owned(),
        ));
        let platform = Arc::new(KickPlatform::new(
            &endpoints,
            Arc::clone(&manager),
            Arc::new(GrantLimiter),
            ledger,
        ));
        let (_viewer_tx, viewer_rx) = watch::channel(ViewerReport::Absent);
        let (_auth_tx, auth_rx) = watch::channel(PollerAuth::Authorized);
        KickIntegrationBundle::new(
            "streamer".to_owned(),
            42,
            platform,
            manager,
            Arc::new(GrantLimiter),
            viewer_rx,
            auth_rx,
        )
        .0
    }

    fn credentials(stored: StoredCredentials) -> Arc<SwitchableCredentials> {
        Arc::new(SwitchableCredentials(Mutex::new(stored)))
    }

    fn row(
        platform: ViewerPlatform,
        channel_id: &str,
        viewer_id: &str,
        expires_in: Option<Duration>,
        origin: BanOrigin,
    ) -> BanLedgerEntry {
        let now = OffsetDateTime::now_utc();
        BanLedgerEntry {
            key: BanLedgerKey {
                platform,
                channel_id: channel_id.to_owned(),
                viewer_id: viewer_id.to_owned(),
            },
            viewer_name: format!("name-{viewer_id}"),
            reason: Some(format!("reason-{viewer_id}")),
            moderator: None,
            banned_at: now - Duration::minutes(5),
            expires_at: expires_in.map(|term| now + term),
            platform_ban_id: None,
            origin,
        }
    }

    async fn listed_viewer_ids(bundle: &KickIntegrationBundle) -> Vec<String> {
        let BanListOutcome::Page(page) = bundle.list_bans(None).await else {
            panic!("expected a page");
        };
        let mut ids: Vec<String> = page
            .entries
            .into_iter()
            .map(|entry| entry.viewer_id)
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn list_bans_pages_every_active_broadcaster_row_as_unbannable() {
        let ledger = Arc::new(MemoryBanLedger::default());
        let issued = row(
            ViewerPlatform::Kick,
            "42",
            "100",
            None,
            BanOrigin::IssuedByForge,
        );
        let observed = row(
            ViewerPlatform::Kick,
            "42",
            "200",
            Some(Duration::hours(1)),
            BanOrigin::Observed,
        );
        ledger.seed(issued.clone());
        ledger.seed(observed.clone());
        ledger.seed(row(
            ViewerPlatform::Kick,
            "43",
            "300",
            None,
            BanOrigin::Observed,
        ));
        ledger.seed(row(
            ViewerPlatform::Twitch,
            "42",
            "400",
            None,
            BanOrigin::Observed,
        ));
        ledger.seed(row(
            ViewerPlatform::Kick,
            "42",
            "500",
            Some(Duration::minutes(-1)),
            BanOrigin::Observed,
        ));
        let bundle = bundle_over(credentials(StoredCredentials::Broadcaster(42)), ledger);

        let BanListOutcome::Page(mut page) = bundle.list_bans(None).await else {
            panic!("expected a page");
        };

        page.entries.sort_by(|a, b| a.viewer_id.cmp(&b.viewer_id));
        assert_eq!(
            page,
            BanPage {
                entries: vec![
                    BanEntry {
                        viewer_id: "100".to_owned(),
                        name: "name-100".to_owned(),
                        reason: Some("reason-100".to_owned()),
                        moderator: None,
                        created_at: Some(issued.banned_at),
                        duration: BanDuration::Permanent,
                        unban: UnbanAbility::Allowed,
                    },
                    BanEntry {
                        viewer_id: "200".to_owned(),
                        name: "name-200".to_owned(),
                        reason: Some("reason-200".to_owned()),
                        moderator: None,
                        created_at: Some(observed.banned_at),
                        duration: BanDuration::Until(observed.expires_at.unwrap()),
                        unban: UnbanAbility::Allowed,
                    },
                ],
                next: None,
            }
        );
    }

    #[tokio::test]
    async fn list_bans_follows_an_account_switch() {
        let ledger = Arc::new(MemoryBanLedger::default());
        ledger.seed(row(
            ViewerPlatform::Kick,
            "42",
            "100",
            None,
            BanOrigin::Observed,
        ));
        ledger.seed(row(
            ViewerPlatform::Kick,
            "43",
            "300",
            None,
            BanOrigin::Observed,
        ));
        let stored = credentials(StoredCredentials::Broadcaster(42));
        let bundle = bundle_over(Arc::clone(&stored), ledger);
        assert_eq!(listed_viewer_ids(&bundle).await, ["100"]);

        stored.switch_to(StoredCredentials::Broadcaster(43));

        assert_eq!(listed_viewer_ids(&bundle).await, ["300"]);
    }

    #[tokio::test]
    async fn list_bans_is_unavailable_without_readable_credentials_or_ledger() {
        for (label, stored, failing, expected) in [
            (
                "signed out",
                StoredCredentials::Absent,
                None,
                BanListUnavailable::NotConnected,
            ),
            (
                "credentials store offline",
                StoredCredentials::Unreadable,
                None,
                BanListUnavailable::Transport,
            ),
            (
                "corrupt credentials",
                StoredCredentials::Corrupt,
                None,
                BanListUnavailable::Transport,
            ),
            (
                "ledger offline",
                StoredCredentials::Broadcaster(42),
                Some(LedgerOp::List),
                BanListUnavailable::Transport,
            ),
        ] {
            let ledger = Arc::new(
                failing.map_or_else(MemoryBanLedger::default, MemoryBanLedger::failing_on),
            );
            ledger.seed(row(
                ViewerPlatform::Kick,
                "42",
                "100",
                None,
                BanOrigin::Observed,
            ));
            let bundle = bundle_over(credentials(stored), ledger);

            assert_eq!(
                bundle.list_bans(None).await,
                BanListOutcome::Unavailable(expected),
                "case: {label}"
            );
        }
    }
}
