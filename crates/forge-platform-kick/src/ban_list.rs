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
