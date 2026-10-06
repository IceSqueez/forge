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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::chat::build_event;

    fn banned_at() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_791_316_800).unwrap()
    }

    fn observed_from_wire(raw: Value) -> Option<BanLedgerEntry> {
        let event = build_event("App\\Events\\UserBannedEvent", raw).unwrap();
        observed_ban(&event.payload, 42, banned_at())
    }

    #[test]
    fn observed_ban_records_viewer_moderator_and_reason_under_the_broadcaster() {
        let entry = observed_from_wire(json!({
            "user": { "id": 77, "username": "Troll" },
            "banned_by": { "id": 2, "username": "ModAlice" },
            "permanent_ban_reason": "spam"
        }));

        assert_eq!(
            entry,
            Some(BanLedgerEntry {
                key: BanLedgerKey {
                    platform: ViewerPlatform::Kick,
                    channel_id: "42".to_owned(),
                    viewer_id: "77".to_owned(),
                },
                viewer_name: "Troll".to_owned(),
                reason: Some("spam".to_owned()),
                moderator: Some("ModAlice".to_owned()),
                banned_at: banned_at(),
                expires_at: None,
                platform_ban_id: None,
                origin: BanOrigin::Observed,
            })
        );
    }

    #[test]
    fn observed_ban_expiry_is_the_ban_time_plus_the_duration_in_seconds() {
        for (duration, expected) in [
            (None, None),
            (Some(1), Some(banned_at() + Duration::seconds(1))),
            (Some(300), Some(banned_at() + Duration::minutes(5))),
            (Some(604_800), Some(banned_at() + Duration::days(7))),
        ] {
            let mut raw = json!({ "user": { "id": 77, "username": "Troll" } });
            if let Some(seconds) = duration {
                raw["duration"] = json!(seconds);
            }

            let entry = observed_from_wire(raw).unwrap();

            assert_eq!(entry.expires_at, expected, "duration {duration:?}");
        }
    }

    #[test]
    fn observed_ban_falls_back_to_the_viewer_id_and_drops_empty_fields() {
        let entry = observed_from_wire(json!({
            "user": { "id": 77, "username": "" },
            "banned_by": { "id": 2, "username": "" },
            "permanent_ban_reason": ""
        }))
        .unwrap();

        assert_eq!(
            (entry.viewer_name.as_str(), entry.moderator, entry.reason),
            ("77", None, None)
        );
    }

    #[test]
    fn observed_ban_without_a_numeric_viewer_id_is_not_recorded() {
        for raw in [
            json!({ "user": { "username": "Troll" } }),
            json!({ "user": { "id": "77", "username": "Troll" } }),
            json!({ "banned_by": { "id": 2, "username": "ModAlice" } }),
        ] {
            assert_eq!(observed_from_wire(raw.clone()), None, "payload {raw}");
        }
    }
}
