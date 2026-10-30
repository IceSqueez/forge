use async_trait::async_trait;
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility, UnbanRefusal,
};
use forge_storage::{BAN_LEDGER_LIST_CAP, BanLedgerEntry};
use forge_types::PlatformId;
use time::OffsetDateTime;

use crate::builtin::YoutubeIntegrationBundle;

fn ban_entry_of(entry: BanLedgerEntry) -> BanEntry {
    BanEntry {
        unban: if entry.platform_ban_id.is_some() {
            UnbanAbility::Allowed
        } else {
            UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge)
        },
        viewer_id: entry.key.viewer_id,
        name: entry.viewer_name,
        reason: entry.reason,
        moderator: entry.moderator,
        created_at: Some(entry.banned_at),
        duration: entry
            .expires_at
            .map_or(BanDuration::Permanent, BanDuration::Until),
    }
}

#[async_trait]
impl BanListSource for YoutubeIntegrationBundle {
    async fn list_bans(&self, _after: Option<&BanPageToken>) -> BanListOutcome {
        let platform = self.platform();
        let broadcaster_channel_id = platform.channel_id();
        if broadcaster_channel_id.is_empty() {
            return BanListOutcome::Unavailable(BanListUnavailable::NotConnected);
        }
        match platform
            .ban_ledger()
            .list_active(
                PlatformId::YouTube,
                broadcaster_channel_id,
                OffsetDateTime::now_utc(),
                BAN_LEDGER_LIST_CAP,
            )
            .await
        {
            Ok(entries) => {
                if entries.len() >= BAN_LEDGER_LIST_CAP {
                    tracing::warn!(
                        cap = BAN_LEDGER_LIST_CAP,
                        "youtube ban list truncated at the ledger cap"
                    );
                }
                BanListOutcome::Page(BanPage {
                    entries: entries.into_iter().map(ban_entry_of).collect(),
                    next: None,
                })
            }
            Err(error) => {
                tracing::warn!(%error, "youtube ban ledger list failed");
                BanListOutcome::Unavailable(BanListUnavailable::Transport)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::{BanLedgerKey, BanLedgerRepo, BanOrigin};
    use time::Duration;

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::ban_ledger_test_support::{LedgerOp, MemoryBanLedger};
    use crate::chat_platform::YoutubePlatform;
    use crate::credentials_manager::YoutubeCredentialsManager;
    use crate::live_chat_id::LiveChatIdHandle;

    const BROADCASTER: &str = "UCbroadcaster";

    fn bundle_over(
        broadcaster_channel_id: &str,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> Arc<YoutubeIntegrationBundle> {
        let manager = Arc::new(YoutubeCredentialsManager::new(
            Arc::new(MockCredentialsRepo::new()),
            crate::endpoints_test_support::unreachable_oauth_client(),
            crate::endpoints_test_support::unreachable_data_api(),
        ));
        let platform = Arc::new(YoutubePlatform::new(
            crate::endpoints_test_support::unreachable_data_api(),
            broadcaster_channel_id.to_owned(),
            Arc::clone(&manager),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            ledger,
        ));
        YoutubeIntegrationBundle::new(broadcaster_channel_id.to_owned(), platform, manager)
    }

    fn row(
        channel_id: &str,
        viewer_id: &str,
        expires_in: Option<Duration>,
        platform_ban_id: Option<&str>,
    ) -> BanLedgerEntry {
        let banned_at = OffsetDateTime::now_utc() - Duration::minutes(5);
        BanLedgerEntry {
            key: BanLedgerKey {
                platform: PlatformId::YouTube,
                channel_id: channel_id.to_owned(),
                viewer_id: viewer_id.to_owned(),
            },
            viewer_name: format!("name-{viewer_id}"),
            reason: None,
            moderator: Some("ModAlice".to_owned()),
            banned_at,
            expires_at: expires_in.map(|term| OffsetDateTime::now_utc() + term),
            platform_ban_id: platform_ban_id.map(str::to_owned),
            origin: if platform_ban_id.is_some() {
                BanOrigin::IssuedByForge
            } else {
                BanOrigin::Observed
            },
        }
    }

    #[tokio::test]
    async fn list_bans_offers_unban_only_for_rows_holding_a_ban_id() {
        let ledger = Arc::new(MemoryBanLedger::default());
        let issued = row(BROADCASTER, "UCissued", None, Some("ban-1"));
        let observed = row(BROADCASTER, "UCobserved", Some(Duration::hours(1)), None);
        ledger.seed(issued.clone());
        ledger.seed(observed.clone());
        ledger.seed(row("UCotherchannel", "UCelsewhere", None, Some("ban-2")));
        let bundle = bundle_over(BROADCASTER, ledger);

        let BanListOutcome::Page(page) = bundle.list_bans(None).await else {
            panic!("expected a page");
        };

        let mut entries = page.entries;
        entries.sort_by(|a, b| a.viewer_id.cmp(&b.viewer_id));
        assert_eq!(
            entries,
            [
                BanEntry {
                    viewer_id: "UCissued".to_owned(),
                    name: "name-UCissued".to_owned(),
                    reason: None,
                    moderator: Some("ModAlice".to_owned()),
                    created_at: Some(issued.banned_at),
                    duration: BanDuration::Permanent,
                    unban: UnbanAbility::Allowed,
                },
                BanEntry {
                    viewer_id: "UCobserved".to_owned(),
                    name: "name-UCobserved".to_owned(),
                    reason: None,
                    moderator: Some("ModAlice".to_owned()),
                    created_at: Some(observed.banned_at),
                    duration: BanDuration::Until(observed.expires_at.unwrap()),
                    unban: UnbanAbility::Refused(UnbanRefusal::BannedOutsideForge),
                },
            ]
        );
        assert_eq!(page.next, None);
    }

    #[tokio::test]
    async fn list_bans_is_unavailable_without_a_channel_or_a_readable_ledger() {
        for (broadcaster, failing, expected) in [
            ("", None, BanListUnavailable::NotConnected),
            (
                BROADCASTER,
                Some(LedgerOp::List),
                BanListUnavailable::Transport,
            ),
        ] {
            let ledger = Arc::new(
                failing.map_or_else(MemoryBanLedger::default, MemoryBanLedger::failing_on),
            );
            ledger.seed(row("", "UCviewer", None, Some("ban-1")));
            let bundle = bundle_over(broadcaster, ledger);

            assert_eq!(
                bundle.list_bans(None).await,
                BanListOutcome::Unavailable(expected)
            );
        }
    }
}
