use serde_json::Value;

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn chat_message_sent_flattens_sender_identity_into_canonical_shape() {
        let out = chat_message_sent(&json!({
            "id": "msg-uuid",
            "content": "hello",
            "sender": {
                "id": 42,
                "username": "viewer",
                "slug": "Viewer Display",
                "identity": { "color": "#00FF00" }
            }
        }));
        assert_eq!(out["message_id"], json!("msg-uuid"));
        assert_eq!(out["content"], json!("hello"));
        assert_eq!(out["sender"]["id"], json!(42));
        assert_eq!(out["sender"]["username"], json!("viewer"));
        assert_eq!(out["sender"]["display_name"], json!("Viewer Display"));
        assert_eq!(out["sender"]["color"], json!("#00FF00"));
    }

    #[test]
    fn chat_message_sent_reply_id_reads_metadata_original_message() {
        let out = chat_message_sent(&json!({
            "id": "m",
            "metadata": { "original_message": { "id": "parent-1" } }
        }));
        assert_eq!(out["reply_to_message_id"], json!("parent-1"));
    }

    #[test]
    fn chat_message_sent_reply_id_is_null_when_not_a_reply() {
        let out = chat_message_sent(&json!({ "id": "m", "metadata": null }));
        assert!(out["reply_to_message_id"].is_null());
    }

    #[test]
    fn chat_message_deleted_maps_message_id_and_deleter_entity() {
        let out = chat_message_deleted(&json!({
            "message": { "id": "msg-999" },
            "deleted_by": { "id": 5, "username": "mod_person" }
        }));
        assert_eq!(out["message_id"], json!("msg-999"));
        assert_eq!(out["deleted_by"]["id"], json!(5));
        assert_eq!(out["deleted_by"]["username"], json!("mod_person"));
    }

    #[test]
    fn moderation_banned_timeout_carries_duration_and_is_not_permanent() {
        let out = moderation_banned(&json!({
            "user": { "id": 77, "username": "bad" },
            "banned_by": { "id": 2, "username": "mod" },
            "duration": 300,
            "permanent_ban_reason": ""
        }));
        assert_eq!(out["banned_user"]["id"], json!(77));
        assert_eq!(out["moderator"]["username"], json!("mod"));
        assert_eq!(out["is_permanent"], json!(false));
        assert_eq!(out["duration_secs"], json!(300));
        assert!(out["reason"].is_null());
    }

    #[test]
    fn moderation_banned_permanent_has_null_duration_and_maps_reason() {
        let out = moderation_banned(&json!({
            "user": { "id": 77, "username": "bad" },
            "banned_by": { "id": 2, "username": "mod" },
            "permanent_ban_reason": "spam"
        }));
        assert_eq!(out["is_permanent"], json!(true));
        assert!(out["duration_secs"].is_null());
        assert_eq!(out["reason"], json!("spam"));
    }

    #[test]
    fn channel_subscribed_takes_first_user_id_and_tier_slug() {
        let out = channel_subscribed(&json!({
            "user_ids": [123, 456],
            "username": "sub",
            "months": 3,
            "subscription": { "slug": "tier1" }
        }));
        assert_eq!(out["subscriber"]["id"], json!(123));
        assert_eq!(out["subscriber"]["username"], json!("sub"));
        assert_eq!(out["months"], json!(3));
        assert_eq!(out["tier"], json!("tier1"));
    }

    #[test]
    fn channel_subscription_gifted_expands_giftees_and_counts_them() {
        let out = channel_subscription_gifted(&json!({
            "gifter_user_id": 200,
            "gifter_username": "gen",
            "gifted_usernames": ["a", "b", "c"],
            "subscription": { "slug": "tier1" }
        }));
        assert_eq!(out["gifter"]["id"], json!(200));
        assert_eq!(out["count"], json!(3));
        assert_eq!(out["giftees"][0]["username"], json!("a"));
        assert!(out["giftees"][0]["id"].is_null());
        assert_eq!(out["tier"], json!("tier1"));
    }

    #[test]
    fn channel_hosted_maps_host_username_and_viewer_count() {
        let out = channel_hosted(&json!({
            "host_username": "hoster",
            "number_viewers": 250
        }));
        assert_eq!(out["host"]["username"], json!("hoster"));
        assert!(out["host"]["id"].is_null());
        assert_eq!(out["viewer_count"], json!(250));
    }

    #[test]
    fn malformed_payloads_yield_null_scalars_without_panic() {
        let empty = json!({});
        assert!(chat_message_sent(&empty)["message_id"].is_null());
        assert!(chat_message_sent(&empty)["sender"]["id"].is_null());
        assert!(chat_message_deleted(&empty)["message_id"].is_null());
        assert!(moderation_banned(&empty)["banned_user"]["id"].is_null());
        assert!(channel_subscribed(&empty)["subscriber"]["id"].is_null());
        assert!(channel_subscription_gifted(&empty)["gifter"]["id"].is_null());
        assert_eq!(channel_subscription_gifted(&empty)["count"], json!(0));
        assert!(channel_hosted(&empty)["host"]["username"].is_null());
    }
}
