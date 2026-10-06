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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use serde_json::json;

    use super::*;

    const BROADCASTER: &str = "UCbroadcaster";
    const PUBLISHED_AT: &str = "2026-10-06T18:30:00Z";

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-10-06T20:00:00Z", &Rfc3339).unwrap()
    }

    fn published_at() -> OffsetDateTime {
        OffsetDateTime::parse(PUBLISHED_AT, &Rfc3339).unwrap()
    }

    fn ban_item() -> Value {
        json!({
            "id": "msg-1",
            "snippet": {
                "type": "userBannedEvent",
                "publishedAt": PUBLISHED_AT,
                "userBannedDetails": {
                    "bannedUserDetails": {
                        "channelId": "UCviewer",
                        "displayName": "Troll"
                    },
                    "banType": "permanent"
                }
            },
            "authorDetails": {
                "channelId": "UCmod",
                "displayName": "ModAlice"
            }
        })
    }

    fn with(mut item: Value, pointer: &str, value: Value) -> Value {
        *item.pointer_mut(pointer).expect("an existing field") = value;
        item
    }

    fn details(item: Value, ban_type: &str, duration: Option<Value>) -> Value {
        let mut item = with(item, "/snippet/userBannedDetails/banType", json!(ban_type));
        if let Some(duration) = duration {
            item["snippet"]["userBannedDetails"]["banDurationSeconds"] = duration;
        }
        item
    }

    #[test]
    fn observed_permanent_ban_becomes_an_observed_row_without_a_ban_id() {
        let entry = observed_ban(&ban_item(), BROADCASTER, now()).unwrap();

        assert_eq!(
            entry,
            BanLedgerEntry {
                key: ban_ledger_key(BROADCASTER, "UCviewer"),
                viewer_name: "Troll".to_owned(),
                reason: None,
                moderator: Some("ModAlice".to_owned()),
                banned_at: published_at(),
                expires_at: None,
                platform_ban_id: None,
                origin: BanOrigin::Observed,
            }
        );
    }

    #[test]
    fn observed_ban_is_skipped_without_a_viewer_or_a_broadcaster() {
        let banned_user = "/snippet/userBannedDetails/bannedUserDetails";
        for (item, broadcaster) in [
            (
                with(ban_item(), &format!("{banned_user}/channelId"), json!("")),
                BROADCASTER,
            ),
            (
                with(ban_item(), &format!("{banned_user}/channelId"), json!(null)),
                BROADCASTER,
            ),
            (
                with(ban_item(), banned_user, json!({"displayName": "Troll"})),
                BROADCASTER,
            ),
            (
                with(ban_item(), "/snippet/userBannedDetails", json!(null)),
                BROADCASTER,
            ),
            (ban_item(), ""),
        ] {
            assert!(
                observed_ban(&item, broadcaster, now()).is_none(),
                "expected skip for {item} on broadcaster {broadcaster:?}"
            );
        }
    }

    #[test]
    fn observed_ban_name_falls_back_to_the_viewer_channel_id() {
        let display_name = "/snippet/userBannedDetails/bannedUserDetails/displayName";
        for missing in [json!(""), json!(null)] {
            let item = with(ban_item(), display_name, missing);

            let entry = observed_ban(&item, BROADCASTER, now()).unwrap();

            assert_eq!(entry.viewer_name, "UCviewer");
        }
    }

    #[test]
    fn observed_ban_without_an_author_has_no_moderator() {
        for author in [
            json!(null),
            json!({"channelId": "UCmod"}),
            json!({"displayName": ""}),
        ] {
            let item = with(ban_item(), "/authorDetails", author);

            let entry = observed_ban(&item, BROADCASTER, now()).unwrap();

            assert_eq!(entry.moderator, None);
        }
    }

    #[test]
    fn observed_ban_time_falls_back_to_now_when_published_at_is_unusable() {
        for published in [
            json!(""),
            json!(null),
            json!("yesterday"),
            json!(1_790_000_000),
        ] {
            let item = with(ban_item(), "/snippet/publishedAt", published.clone());

            let entry = observed_ban(&item, BROADCASTER, now()).unwrap();

            assert_eq!(entry.banned_at, now(), "publishedAt {published}");
        }
    }

    #[test]
    fn observed_temporary_ban_expires_its_duration_after_publication() {
        for (ban_type, duration) in [
            ("temporary", json!(600)),
            ("temporary", json!("600")),
            ("TEMPORARY", json!(600)),
        ] {
            let item = details(ban_item(), ban_type, Some(duration.clone()));

            let entry = observed_ban(&item, BROADCASTER, now()).unwrap();

            assert_eq!(
                entry.expires_at,
                Some(published_at() + Duration::seconds(600)),
                "{ban_type} {duration}"
            );
        }
    }

    #[test]
    fn observed_temporary_ban_without_a_usable_duration_is_skipped() {
        for duration in [
            None,
            Some(json!(null)),
            Some(json!("")),
            Some(json!("ten minutes")),
            Some(json!(-5)),
            Some(json!(1.5)),
        ] {
            let item = details(ban_item(), "temporary", duration.clone());

            assert!(
                observed_ban(&item, BROADCASTER, now()).is_none(),
                "duration {duration:?}"
            );
        }
    }

    #[test]
    fn observed_permanent_ban_ignores_a_duration() {
        let item = details(ban_item(), "permanent", Some(json!(600)));

        let entry = observed_ban(&item, BROADCASTER, now()).unwrap();

        assert_eq!(entry.expires_at, None);
    }

    #[test]
    fn expiry_after_adds_the_term_and_rejects_overflow() {
        let max_seconds = u64::try_from(i64::MAX).unwrap();
        for (seconds, expected) in [
            (0, Some(now())),
            (600, Some(now() + Duration::seconds(600))),
            (max_seconds, None),
            (max_seconds + 1, None),
            (u64::MAX, None),
        ] {
            assert_eq!(expiry_after(now(), seconds), expected, "{seconds} seconds");
        }
    }
}
