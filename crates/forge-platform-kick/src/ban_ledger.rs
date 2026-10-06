use forge_storage::{BanLedgerEntry, BanLedgerKey, BanOrigin, ViewerPlatform};
use serde_json::Value;
use time::{Duration, OffsetDateTime};

use crate::payload_fields::{entity, moderation};

pub(crate) const USER_BANNED_EVENT_KIND: &str = "kick.moderation.banned";

pub(crate) fn ban_ledger_key(broadcaster_user_id: u64, viewer_user_id: &str) -> BanLedgerKey {
    BanLedgerKey {
        platform: ViewerPlatform::Kick,
        channel_id: broadcaster_user_id.to_string(),
        viewer_id: viewer_user_id.to_owned(),
    }
}

pub(crate) fn timeout_term_minutes(minutes: u32) -> Duration {
    Duration::minutes(i64::from(minutes))
}

pub(crate) fn issued_ban(
    key: BanLedgerKey,
    viewer_name: String,
    reason: Option<String>,
    banned_at: OffsetDateTime,
    term: Option<Duration>,
) -> BanLedgerEntry {
    BanLedgerEntry {
        key,
        viewer_name,
        reason,
        moderator: None,
        banned_at,
        expires_at: term.and_then(|term| banned_at.checked_add(term)),
        platform_ban_id: None,
        origin: BanOrigin::IssuedByForge,
    }
}

pub(crate) fn observed_ban(
    payload: &Value,
    broadcaster_user_id: u64,
    banned_at: OffsetDateTime,
) -> Option<BanLedgerEntry> {
    let banned_user = payload.get(moderation::BANNED_USER)?;
    let viewer_id = banned_user.get(entity::ID)?.as_u64()?.to_string();
    let expires_at = match payload
        .get(moderation::DURATION_SECS)
        .and_then(Value::as_u64)
    {
        Some(seconds) => {
            Some(banned_at.checked_add(Duration::seconds(i64::try_from(seconds).ok()?))?)
        }
        None => None,
    };
    Some(BanLedgerEntry {
        viewer_name: non_empty_str(banned_user, entity::USERNAME)
            .unwrap_or(&viewer_id)
            .to_owned(),
        key: ban_ledger_key(broadcaster_user_id, &viewer_id),
        reason: non_empty_str(payload, moderation::REASON).map(str::to_owned),
        moderator: payload
            .get(moderation::MODERATOR)
            .and_then(|moderator| non_empty_str(moderator, entity::USERNAME))
            .map(str::to_owned),
        banned_at,
        expires_at,
        platform_ban_id: None,
        origin: BanOrigin::Observed,
    })
}

fn non_empty_str<'a>(object: &'a Value, field: &str) -> Option<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}
