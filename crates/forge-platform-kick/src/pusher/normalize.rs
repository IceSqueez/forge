use forge_types::{ChatPayload, ChatSegment, ChatViewer, ModerationMarks, UserBadge};
use serde_json::Value;
use tracing::debug;

use crate::payload_fields::{chat, entity, host, moderation, subscription, subscription_gift};

fn u64_field(v: &Value, key: &str) -> Option<u64> {
    v.get(key).and_then(Value::as_u64)
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

fn entity_json(id: Option<u64>, username: Option<String>) -> Value {
    serde_json::json!({
        (entity::ID): id,
        (entity::USERNAME): username,
    })
}

fn identity_badges(identity: Option<&Value>) -> Vec<UserBadge> {
    let badges = match identity
        .and_then(|i| i.get("badges"))
        .and_then(Value::as_array)
    {
        Some(b) => b,
        None => return Vec::new(),
    };
    badges
        .iter()
        .filter_map(|badge| match str_field(badge, "type").as_deref() {
            Some("broadcaster") => Some(UserBadge::Broadcaster),
            Some("moderator") => Some(UserBadge::Moderator),
            Some("vip") => Some(UserBadge::Vip),
            Some("subscriber") => Some(UserBadge::Subscriber {
                months: u64_field(badge, "count").unwrap_or(0) as u32,
            }),
            Some("founder") => Some(UserBadge::Founder),
            Some("bot") => Some(UserBadge::Bot),
            Some("verified") => Some(UserBadge::Partner),
            Some("sub_gifter" | "og" | "staff" | "sidekick" | "trainwreckstv") => None,
            Some(other) => {
                debug!(badge_type = %other, "unrecognized kick identity badge type; dropping");
                None
            }
            None => None,
        })
        .collect()
}

pub(crate) fn chat_message_sent(raw: &Value) -> Value {
    let sender = raw.get("sender");
    let sender_id = sender.and_then(|s| u64_field(s, "id"));
    let sender_username = sender.and_then(|s| str_field(s, "username"));
    let sender_display_name = sender.and_then(|s| str_field(s, "slug"));
    let color = sender
        .and_then(|s| s.get("identity"))
        .and_then(|i| str_field(i, "color"));

    let reply_to_message_id = raw
        .get("metadata")
        .and_then(|m| m.get("original_message"))
        .and_then(|o| str_field(o, "id"));

    serde_json::json!({
        (chat::MESSAGE_ID): str_field(raw, "id"),
        (chat::CONTENT): str_field(raw, "content"),
        (chat::REPLY_TO_MESSAGE_ID): reply_to_message_id,
        (chat::SENDER): {
            (entity::ID): sender_id,
            (entity::USERNAME): sender_username,
            (entity::DISPLAY_NAME): sender_display_name,
            (chat::COLOR): color,
        },
    })
}

pub(crate) fn chat_message_chat_payload(raw: &Value) -> ChatPayload {
    let sender = raw.get("sender");
    let identity = sender.and_then(|s| s.get("identity"));
    let username = sender.and_then(|s| str_field(s, "username"));
    let display_name = sender.and_then(|s| str_field(s, "slug"));
    let author_color = non_empty(identity.and_then(|i| str_field(i, "color")));

    ChatPayload {
        platform_msg_id: str_field(raw, "id").unwrap_or_default(),
        author: display_name.or(username).unwrap_or_default(),
        author_color,
        segments: vec![ChatSegment::Text {
            text: str_field(raw, "content").unwrap_or_default(),
        }],
        badges: identity_badges(identity),
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn chat_message_viewer(raw: &Value) -> Option<ChatViewer> {
    let sender = raw.get("sender")?;
    let id = u64_field(sender, "id")?;
    let username = str_field(sender, "username").filter(|name| !name.is_empty())?;
    Some(ChatViewer::new(id.to_string(), username))
}

pub(crate) fn chat_message_deleted(raw: &Value) -> Value {
    let message_id = raw.get("message").and_then(|m| str_field(m, "id"));
    let deleted_by = raw.get("deleted_by");
    let deleted_by_id = deleted_by.and_then(|d| u64_field(d, "id"));
    let deleted_by_username = deleted_by.and_then(|d| str_field(d, "username"));

    serde_json::json!({
        (chat::MESSAGE_ID): message_id,
        (chat::DELETED_BY): entity_json(deleted_by_id, deleted_by_username),
    })
}

pub(crate) fn moderation_banned(raw: &Value) -> Value {
    let banned_user = raw.get("user");
    let banned_user_id = banned_user.and_then(|u| u64_field(u, "id"));
    let banned_user_username = banned_user.and_then(|u| str_field(u, "username"));

    let moderator = raw.get("banned_by");
    let moderator_id = moderator.and_then(|m| u64_field(m, "id"));
    let moderator_username = moderator.and_then(|m| str_field(m, "username"));

    let duration_secs = u64_field(raw, "duration");
    let reason = non_empty(str_field(raw, "permanent_ban_reason"));
    let is_permanent = duration_secs.is_none();

    serde_json::json!({
        (moderation::BANNED_USER): entity_json(banned_user_id, banned_user_username),
        (moderation::MODERATOR): entity_json(moderator_id, moderator_username),
        (moderation::IS_PERMANENT): is_permanent,
        (moderation::DURATION_SECS): duration_secs,
        (moderation::REASON): reason,
    })
}

pub(crate) fn channel_subscribed(raw: &Value) -> Value {
    let subscriber_id = raw
        .get("user_ids")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_u64);
    let subscriber_username = str_field(raw, "username");
    let months = u64_field(raw, "months");
    let tier = raw.get("subscription").and_then(|s| str_field(s, "slug"));

    serde_json::json!({
        (subscription::SUBSCRIBER): entity_json(subscriber_id, subscriber_username),
        (subscription::MONTHS): months,
        (subscription::TIER): tier,
    })
}

pub(crate) fn channel_subscription_gifted(raw: &Value) -> Value {
    let gifter_id = u64_field(raw, "gifter_user_id");
    let gifter_username = str_field(raw, "gifter_username");

    let giftees: Vec<Value> = raw
        .get("gifted_usernames")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(|name| entity_json(None, Some(name.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let count = giftees.len() as u64;

    let tier = raw.get("subscription").and_then(|s| str_field(s, "slug"));

    serde_json::json!({
        (subscription_gift::GIFTER): entity_json(gifter_id, gifter_username),
        (subscription_gift::GIFTEES): giftees,
        (subscription_gift::COUNT): count,
        (subscription_gift::TIER): tier,
    })
}

pub(crate) fn channel_hosted(raw: &Value) -> Value {
    let host_username = str_field(raw, "host_username");
    let viewer_count = u64_field(raw, "number_viewers");

    serde_json::json!({
        (host::HOST): entity_json(None, host_username),
        (host::VIEWER_COUNT): viewer_count,
    })
}
