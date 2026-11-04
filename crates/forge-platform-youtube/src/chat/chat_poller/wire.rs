use super::*;

pub(super) fn item_type(item: &serde_json::Value) -> &str {
    item.get("snippet")
        .and_then(|s| s.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

pub(super) fn sleep_duration(polling_interval_millis: u64, floor_ms: u64) -> Duration {
    Duration::from_millis(polling_interval_millis).max(Duration::from_millis(floor_ms))
}

pub(super) fn extract_author(author_details: Option<&serde_json::Value>) -> String {
    author_details
        .and_then(|ad| ad.get("displayName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

pub(super) fn extract_author_channel_id(
    author_details: Option<&serde_json::Value>,
) -> Option<String> {
    author_details
        .and_then(|ad| ad.get("channelId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn extract_author_avatar_url(
    author_details: Option<&serde_json::Value>,
) -> Option<String> {
    author_details
        .and_then(|ad| ad.get("profileImageUrl"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

pub(super) fn entity_json(
    channel_id: Option<String>,
    display_name: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        (entity::CHANNEL_ID): channel_id,
        (entity::DISPLAY_NAME): display_name,
    })
}

pub(super) fn author_json(author_details: Option<&serde_json::Value>) -> serde_json::Value {
    entity_json(
        extract_author_channel_id(author_details),
        non_empty(extract_author(author_details)),
    )
}

pub(super) fn extract_badges(author_details: Option<&serde_json::Value>) -> Vec<UserBadge> {
    let ad = match author_details {
        Some(v) => v,
        None => return Vec::new(),
    };
    let mut badges = Vec::new();
    if ad
        .get("isChatOwner")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Broadcaster);
    }
    if ad
        .get("isChatModerator")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Moderator);
    }
    if ad
        .get("isChatSponsor")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        badges.push(UserBadge::Member {
            level: "member".to_owned(),
        });
    }
    badges
}
