use async_trait::async_trait;
use forge_platform_core::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility, UnbanRefusal,
};
use forge_storage::{BAN_LEDGER_LIST_CAP, BanLedgerEntry, ViewerPlatform};
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
                ViewerPlatform::YouTube,
                broadcaster_channel_id,
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
                tracing::warn!(%error, "youtube ban ledger list failed");
                BanListOutcome::Unavailable(BanListUnavailable::Transport)
            }
        }
    }
}
