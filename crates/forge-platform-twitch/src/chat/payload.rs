use forge_types::{ChatEventDetail, ChatPayload, ChatSegment, ModerationMarks, UserBadge};

pub(crate) fn map_fragments(fragments: Option<&serde_json::Value>) -> Vec<ChatSegment> {
    let arr = match fragments.and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|frag| {
            let frag_type = frag.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let text = frag
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            match frag_type {
                "text" => Some(ChatSegment::Text { text }),
                "emote" => {
                    let id = frag
                        .get("emote")
                        .and_then(|e| e.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    Some(ChatSegment::Emote { id, name: text })
                }
                "mention" => {
                    let username = frag
                        .get("mention")
                        .and_then(|m| m.get("user_login"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_owned();
                    Some(ChatSegment::Mention { username })
                }
                "cheermote" => {
                    let id = frag
                        .get("cheermote")
                        .and_then(|c| c.get("prefix"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned();
                    if id.is_empty() {
                        None
                    } else {
                        Some(ChatSegment::Emote { id, name: text })
                    }
                }
                _ => None,
            }
        })
        .collect()
}

pub(crate) fn map_badges(badges: Option<&serde_json::Value>) -> Vec<UserBadge> {
    let arr = match badges.and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|badge| {
            let set_id = badge.get("set_id").and_then(|v| v.as_str()).unwrap_or("");
            let info = badge.get("info").and_then(|v| v.as_str()).unwrap_or("");
            match set_id {
                "broadcaster" => Some(UserBadge::Broadcaster),
                "moderator" => Some(UserBadge::Moderator),
                "vip" => Some(UserBadge::Vip),
                "subscriber" => {
                    let months: u32 = info.parse().unwrap_or(0);
                    Some(UserBadge::Subscriber { months })
                }
                _ => None,
            }
        })
        .collect()
}

pub(crate) fn tier_str_to_u8(tier: &str) -> u8 {
    match tier {
        "1000" => 1,
        "2000" => 2,
        "3000" => 3,
        _ => 0,
    }
}

pub(crate) fn build_chat_message_chat_payload(event_data: &serde_json::Value) -> ChatPayload {
    let platform_msg_id = event_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let author = event_data
        .get("chatter_user_name")
        .and_then(|v| v.as_str())
        .or_else(|| {
            event_data
                .get("chatter_user_login")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_owned();
    let author_color = event_data
        .get("color")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let segments = map_fragments(event_data.get("message").and_then(|m| m.get("fragments")));
    let badges = map_badges(event_data.get("badges"));
    ChatPayload {
        platform_msg_id,
        author,
        author_color,
        segments,
        badges,
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn build_subscribe_chat_payload(
    event_data: &serde_json::Value,
    frame_msg_id: &str,
) -> ChatPayload {
    let author = event_data
        .get("user_name")
        .and_then(|v| v.as_str())
        .or_else(|| event_data.get("user_login").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_owned();
    let tier = tier_str_to_u8(
        event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    ChatPayload {
        platform_msg_id: format!("twitch.support.subscriber:{frame_msg_id}"),
        author,
        author_color: None,
        segments: vec![ChatSegment::Text {
            text: format!("subscribed at tier {tier}"),
        }],
        badges: vec![UserBadge::Subscriber { months: 1 }],
        is_event: true,
        event_detail: Some(ChatEventDetail::Subscription {
            tier,
            months: None,
            message: None,
        }),
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn build_resubscribe_chat_payload(
    event_data: &serde_json::Value,
    frame_msg_id: &str,
) -> ChatPayload {
    let author = event_data
        .get("user_name")
        .and_then(|v| v.as_str())
        .or_else(|| event_data.get("user_login").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_owned();
    let tier = tier_str_to_u8(
        event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let months = event_data
        .get("cumulative_months")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let sub_message = event_data
        .get("message")
        .and_then(|m| m.get("text"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let segments = sub_message
        .as_deref()
        .map(|text| {
            vec![ChatSegment::Text {
                text: text.to_owned(),
            }]
        })
        .unwrap_or_default();
    ChatPayload {
        platform_msg_id: format!("twitch.support.resubscriber:{frame_msg_id}"),
        author,
        author_color: None,
        segments,
        badges: vec![UserBadge::Subscriber {
            months: months.unwrap_or(1),
        }],
        is_event: true,
        event_detail: Some(ChatEventDetail::Subscription {
            tier,
            months,
            message: sub_message,
        }),
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn build_gift_sub_chat_payload(
    event_data: &serde_json::Value,
    frame_msg_id: &str,
) -> ChatPayload {
    let is_anonymous = event_data
        .get("is_anonymous")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let author = if is_anonymous {
        "anonymous".to_owned()
    } else {
        event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .or_else(|| event_data.get("user_login").and_then(|v| v.as_str()))
            .unwrap_or("anonymous")
            .to_owned()
    };
    let tier = tier_str_to_u8(
        event_data
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    ChatPayload {
        platform_msg_id: format!("twitch.support.gift_sub:{frame_msg_id}"),
        author,
        author_color: None,
        segments: vec![ChatSegment::Text {
            text: format!("gifted a tier {tier} subscription"),
        }],
        badges: vec![],
        is_event: true,
        event_detail: Some(ChatEventDetail::Subscription {
            tier,
            months: None,
            message: None,
        }),
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn build_cheer_chat_payload(
    event_data: &serde_json::Value,
    frame_msg_id: &str,
) -> ChatPayload {
    let is_anonymous = event_data
        .get("is_anonymous")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let author = if is_anonymous {
        "anonymous".to_owned()
    } else {
        event_data
            .get("user_name")
            .and_then(|v| v.as_str())
            .or_else(|| event_data.get("user_login").and_then(|v| v.as_str()))
            .unwrap_or("anonymous")
            .to_owned()
    };
    let bits = event_data.get("bits").and_then(|v| v.as_u64()).unwrap_or(0);
    let message = event_data
        .get("message")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    let segments = message
        .as_deref()
        .map(|text| {
            vec![ChatSegment::Text {
                text: text.to_owned(),
            }]
        })
        .unwrap_or_default();
    ChatPayload {
        platform_msg_id: format!("twitch.support.cheer:{frame_msg_id}"),
        author,
        author_color: None,
        segments,
        badges: vec![],
        is_event: true,
        event_detail: Some(ChatEventDetail::SuperChat {
            amount_micros: bits * 10_000,
            currency: "BITS".to_owned(),
            message,
        }),
        moderation: ModerationMarks::default(),
    }
}

pub(crate) fn build_raid_chat_payload(
    event_data: &serde_json::Value,
    frame_msg_id: &str,
) -> ChatPayload {
    let author = event_data
        .get("from_broadcaster_user_name")
        .and_then(|v| v.as_str())
        .or_else(|| {
            event_data
                .get("from_broadcaster_user_login")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_owned();
    let viewer_count = event_data
        .get("viewers")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    ChatPayload {
        platform_msg_id: format!("twitch.channel.raid_received:{frame_msg_id}"),
        author,
        author_color: None,
        segments: vec![],
        badges: vec![],
        is_event: true,
        event_detail: Some(ChatEventDetail::Raid { viewer_count }),
        moderation: ModerationMarks::default(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_types::ChatEventDetail;

    use super::*;

    #[test]
    fn map_fragments_dispatches_per_fragment_type() {
        let cases = vec![
            (
                serde_json::json!([{"type": "emote", "text": "KEKW", "emote": {"id": "12345"}}]),
                vec![ChatSegment::Emote {
                    id: "12345".to_owned(),
                    name: "KEKW".to_owned(),
                }],
            ),
            (
                serde_json::json!([{"type": "mention", "text": "@foo", "mention": {"user_login": "foo"}}]),
                vec![ChatSegment::Mention {
                    username: "foo".to_owned(),
                }],
            ),
            (
                serde_json::json!([{"type": "text", "text": "hello"}]),
                vec![ChatSegment::Text {
                    text: "hello".to_owned(),
                }],
            ),
            (
                serde_json::json!([{"type": "cheermote", "text": "Cheer100", "cheermote": {"prefix": "Cheer", "bits": 100, "tier": 1}}]),
                vec![ChatSegment::Emote {
                    id: "Cheer".to_owned(),
                    name: "Cheer100".to_owned(),
                }],
            ),
            (
                serde_json::json!([{"type": "future_unknown", "text": "x"}]),
                vec![],
            ),
            (
                serde_json::json!([{"type": "cheermote", "text": "x", "cheermote": {}}]),
                vec![],
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(map_fragments(Some(&input)), expected, "input={input}");
        }
    }

    #[test]
    fn map_fragments_none_returns_empty() {
        assert!(map_fragments(None).is_empty());
    }

    #[test]
    fn map_badges_broadcaster() {
        let badges = serde_json::json!([{ "set_id": "broadcaster", "id": "1", "info": "" }]);
        assert_eq!(map_badges(Some(&badges)), vec![UserBadge::Broadcaster]);
    }

    #[test]
    fn map_badges_moderator_and_vip() {
        let badges = serde_json::json!([
            { "set_id": "moderator", "id": "1", "info": "" },
            { "set_id": "vip", "id": "1", "info": "" }
        ]);
        let result = map_badges(Some(&badges));
        assert_eq!(result, vec![UserBadge::Moderator, UserBadge::Vip]);
    }

    #[test]
    fn map_badges_subscriber_with_months() {
        let badges = serde_json::json!([{ "set_id": "subscriber", "id": "3012", "info": "36" }]);
        assert_eq!(
            map_badges(Some(&badges)),
            vec![UserBadge::Subscriber { months: 36 }]
        );
    }

    #[test]
    fn map_badges_skips_unknown_set_ids() {
        let badges = serde_json::json!([
            { "set_id": "partner", "id": "1", "info": "" },
            { "set_id": "moderator", "id": "1", "info": "" }
        ]);
        let result = map_badges(Some(&badges));
        assert_eq!(result, vec![UserBadge::Moderator]);
    }

    #[test]
    fn tier_str_to_u8_known_values() {
        assert_eq!(tier_str_to_u8("1000"), 1);
        assert_eq!(tier_str_to_u8("2000"), 2);
        assert_eq!(tier_str_to_u8("3000"), 3);
    }

    #[test]
    fn tier_str_to_u8_unknown_returns_zero() {
        assert_eq!(tier_str_to_u8(""), 0);
        assert_eq!(tier_str_to_u8("4000"), 0);
    }

    #[test]
    fn chat_message_with_color_populates_author_color() {
        let event_data = serde_json::json!({
            "message_id": "x",
            "chatter_user_name": "Viewer",
            "chatter_user_login": "viewer",
            "color": "#FF0000",
            "message": { "text": "hi", "fragments": [] },
            "badges": []
        });
        let payload = build_chat_message_chat_payload(&event_data);
        assert_eq!(payload.author_color, Some("#FF0000".to_owned()));
    }

    #[test]
    fn chat_message_without_color_has_none_author_color() {
        let event_data = serde_json::json!({
            "message_id": "x",
            "chatter_user_name": "Viewer",
            "chatter_user_login": "viewer",
            "color": "",
            "message": { "text": "hi", "fragments": [] },
            "badges": []
        });
        let payload = build_chat_message_chat_payload(&event_data);
        assert_eq!(payload.author_color, None);
    }

    #[test]
    fn cheer_amount_encodes_bits_as_micros() {
        let event_data = serde_json::json!({
            "user_name": "cheerer",
            "user_login": "cheerer",
            "is_anonymous": false,
            "bits": 100u64,
            "message": "PogChamp"
        });
        let payload = build_cheer_chat_payload(&event_data, "meta-001");
        match payload.event_detail.unwrap() {
            ChatEventDetail::SuperChat {
                amount_micros,
                currency,
                ..
            } => {
                assert_eq!(amount_micros, 1_000_000);
                assert_eq!(currency, "BITS");
            }
            other => panic!("expected SuperChat, got {other:?}"),
        }
    }

    #[test]
    fn raid_viewer_count_comes_from_viewers_field() {
        let event_data = serde_json::json!({
            "from_broadcaster_user_id": "666",
            "from_broadcaster_user_login": "big_streamer",
            "from_broadcaster_user_name": "BigStreamer",
            "viewers": 250u64
        });
        let payload = build_raid_chat_payload(&event_data, "meta-002");
        assert_eq!(payload.author, "BigStreamer");
        match payload.event_detail.unwrap() {
            ChatEventDetail::Raid { viewer_count } => assert_eq!(viewer_count, 250),
            other => panic!("expected Raid, got {other:?}"),
        }
    }

    #[test]
    fn subscribe_payload_is_event_with_subscription_detail() {
        let event_data = serde_json::json!({
            "user_id": "111",
            "user_login": "newbie",
            "user_name": "Newbie",
            "tier": "1000",
            "is_gift": false
        });
        let payload = build_subscribe_chat_payload(&event_data, "meta-003");
        assert!(payload.is_event);
        assert_eq!(payload.author, "Newbie");
        match payload.event_detail.unwrap() {
            ChatEventDetail::Subscription {
                tier,
                months,
                message,
            } => {
                assert_eq!(tier, 1);
                assert_eq!(months, None);
                assert_eq!(message, None);
            }
            other => panic!("expected Subscription, got {other:?}"),
        }
    }

    #[test]
    fn gift_sub_anonymous_author_is_anonymous() {
        let event_data = serde_json::json!({
            "user_id": "0",
            "user_login": "",
            "user_name": "",
            "tier": "1000",
            "is_anonymous": true,
            "total": 1
        });
        let payload = build_gift_sub_chat_payload(&event_data, "meta-004");
        assert_eq!(payload.author, "anonymous");
    }
}
