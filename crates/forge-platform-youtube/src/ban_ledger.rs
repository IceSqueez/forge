use forge_storage::{BanLedgerEntry, BanLedgerKey, BanOrigin, ViewerPlatform};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

pub(crate) const USER_BANNED_EVENT_TYPE: &str = "userBannedEvent";
const TEMPORARY_BAN_TYPE: &str = "temporary";

pub(crate) fn ban_ledger_key(
    broadcaster_channel_id: &str,
    viewer_channel_id: &str,
) -> BanLedgerKey {
    BanLedgerKey {
        platform: ViewerPlatform::YouTube,
        channel_id: broadcaster_channel_id.to_owned(),
        viewer_id: viewer_channel_id.to_owned(),
    }
}

pub(crate) fn expiry_after(banned_at: OffsetDateTime, seconds: u64) -> Option<OffsetDateTime> {
    banned_at.checked_add(Duration::seconds(i64::try_from(seconds).ok()?))
}

pub(crate) fn observed_ban(
    item: &Value,
    broadcaster_channel_id: &str,
    now: OffsetDateTime,
) -> Option<BanLedgerEntry> {
    if broadcaster_channel_id.is_empty() {
        return None;
    }
    let snippet = item.get("snippet")?;
    let details = snippet.get("userBannedDetails")?;
    let banned_user = details.get("bannedUserDetails")?;
    let viewer_id = non_empty_str(banned_user, "channelId")?;
    let banned_at = non_empty_str(snippet, "publishedAt")
        .and_then(|raw| OffsetDateTime::parse(raw, &Rfc3339).ok())
        .unwrap_or(now);
    let is_temporary = non_empty_str(details, "banType")
        .is_some_and(|ban_type| ban_type.eq_ignore_ascii_case(TEMPORARY_BAN_TYPE));
    let expires_at = if is_temporary {
        Some(expiry_after(banned_at, ban_duration_seconds(details)?)?)
    } else {
        None
    };
    Some(BanLedgerEntry {
        key: ban_ledger_key(broadcaster_channel_id, viewer_id),
        viewer_name: non_empty_str(banned_user, "displayName")
            .unwrap_or(viewer_id)
            .to_owned(),
        reason: None,
        moderator: item
            .get("authorDetails")
            .and_then(|author| non_empty_str(author, "displayName"))
            .map(str::to_owned),
        banned_at,
        expires_at,
        platform_ban_id: None,
        origin: BanOrigin::Observed,
    })
}

fn ban_duration_seconds(details: &Value) -> Option<u64> {
    let raw = details.get("banDurationSeconds")?;
    raw.as_u64()
        .or_else(|| raw.as_str().and_then(|text| text.parse().ok()))
}

fn non_empty_str<'a>(object: &'a Value, field: &str) -> Option<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}
