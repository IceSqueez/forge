use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::EventId;

/// Attached under `Event::payload["_chat"]`; the `_` prefix is reserved for forge-internal keys (ArgStack keys never start with `_`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatPayload {
    pub platform_msg_id: String,
    pub author: String,
    pub author_color: Option<String>,
    pub segments: Vec<ChatSegment>,
    pub badges: Vec<UserBadge>,
    pub is_event: bool,
    pub event_detail: Option<ChatEventDetail>,
    #[serde(default)]
    pub moderation: ModerationMarks,
}

impl ChatPayload {
    /// Call sites must use this constant rather than the bare string to avoid silent drift.
    pub const KEY: &'static str = "_chat";

    /// `None` unless `s` is exactly 6 hex digits, with or without a leading `#`.
    pub fn parse_color(s: &str) -> Option<[u8; 3]> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r, g, b])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatModerationPayload {
    pub action: ChatModerationAction,
}

impl ChatModerationPayload {
    /// Call sites must use this constant rather than the bare string to avoid silent drift.
    pub const KEY: &'static str = "_chat_mod";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatModerationAction {
    DeleteMessage { message_id: String },
    RemoveUser { user_name: String, timeout: bool },
    ClearChat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReply {
    pub parent_author: String,
    pub parent_text: String,
}

impl ChatReply {
    pub const KEY: &'static str = "_chat_reply";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ChatSource {
    Twitch,
    YouTube,
    Kick,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedChatRow {
    pub id: String,
    pub event_id: EventId,
    pub source: ChatSource,
    #[serde(with = "time::serde::rfc3339")]
    pub received_at: OffsetDateTime,
    pub author: String,
    pub author_color: Option<[u8; 3]>,
    pub body_segments: Vec<ChatSegment>,
    pub badges: Vec<UserBadge>,
    pub is_event: bool,
    pub event_detail: Option<ChatEventDetail>,
    pub moderation: ModerationMarks,
}

impl UnifiedChatRow {
    /// Concatenates `Text` and `Mention` segments (as `@username`) for message rendering.
    pub fn display_text(&self) -> String {
        let mut out = String::new();
        for segment in &self.body_segments {
            match segment {
                ChatSegment::Text { text } => out.push_str(text),
                ChatSegment::Mention { username } => {
                    out.push('@');
                    out.push_str(username);
                }
                _ => {}
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatSegment {
    Text { text: String },
    Emote { id: String, name: String },
    Link { url: String, display: String },
    Mention { username: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UserBadge {
    Broadcaster,
    Moderator,
    Vip,
    Subscriber { months: u32 },
    Member { level: String },
    Bot,
    Partner,
    Premium,
    Founder,
    Turbo,
    HypeTrain,
    Bits { amount: u32 },
    BitsLeader { rank: u32 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModerationMarks {
    pub deleted: bool,
    pub timed_out: bool,
    pub banned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatEventDetail {
    Subscription {
        tier: u8,
        months: Option<u32>,
        message: Option<String>,
    },
    Raid {
        viewer_count: u64,
    },
    Cheer {
        bits: u64,
        message: Option<String>,
    },
    SuperChat {
        amount_micros: u64,
        currency: String,
        message: Option<String>,
    },
    NewMember {
        level: String,
    },
    MemberMilestone {
        months: u32,
        message: Option<String>,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::{from_str, to_string};
    use time::OffsetDateTime;

    use super::*;
    use crate::EventId;

    fn minimal_payload(msg_id: &str) -> ChatPayload {
        ChatPayload {
            platform_msg_id: msg_id.to_string(),
            author: "user".to_string(),
            author_color: None,
            segments: vec![],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    #[test]
    fn chat_payload_roundtrips_minimal() {
        let payload = minimal_payload("abc123");
        let json = to_string(&payload).unwrap();
        let back: ChatPayload = from_str(&json).unwrap();
        assert_eq!(back, payload);
    }

    #[test]
    fn chat_payload_roundtrips_full() {
        let payload = ChatPayload {
            platform_msg_id: "full-msg".to_string(),
            author: "streamer".to_string(),
            author_color: Some("#FF0000".to_string()),
            segments: vec![
                ChatSegment::Text {
                    text: "wow ".to_string(),
                },
                ChatSegment::Emote {
                    id: "123".to_string(),
                    name: "KEKW".to_string(),
                },
            ],
            badges: vec![UserBadge::Moderator, UserBadge::Subscriber { months: 12 }],
            is_event: true,
            event_detail: Some(ChatEventDetail::SuperChat {
                amount_micros: 5_000_000,
                currency: "USD".to_string(),
                message: Some("amazing!".to_string()),
            }),
            moderation: ModerationMarks {
                deleted: false,
                timed_out: false,
                banned: false,
            },
        };
        let json = to_string(&payload).unwrap();
        let back: ChatPayload = from_str(&json).unwrap();
        assert_eq!(back, payload);
    }

    #[test]
    fn parse_color_accepts_valid_and_rejects_invalid_inputs() {
        for valid in ["#FF00AA", "FF00AA", "#ff00aa"] {
            assert_eq!(
                ChatPayload::parse_color(valid),
                Some([0xFF, 0x00, 0xAA]),
                "should accept {valid}"
            );
        }
        for invalid in ["FFF", "#XXYYZZ", ""] {
            assert_eq!(
                ChatPayload::parse_color(invalid),
                None,
                "should reject {invalid:?}"
            );
        }
    }

    fn make_row(segments: Vec<ChatSegment>) -> UnifiedChatRow {
        UnifiedChatRow {
            id: "test-id".to_string(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: OffsetDateTime::now_utc(),
            author: "testuser".to_string(),
            author_color: None,
            body_segments: segments,
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    #[test]
    fn display_text_renders_mentions() {
        let cases = [
            (
                vec![
                    ChatSegment::Text {
                        text: "hey ".to_string(),
                    },
                    ChatSegment::Mention {
                        username: "foo".to_string(),
                    },
                    ChatSegment::Text {
                        text: " gg".to_string(),
                    },
                    ChatSegment::Emote {
                        id: "1".to_string(),
                        name: "KEKW".to_string(),
                    },
                    ChatSegment::Link {
                        url: "https://x".to_string(),
                        display: "x".to_string(),
                    },
                ],
                "hey @foo gg",
            ),
            (
                vec![ChatSegment::Mention {
                    username: "solo".to_string(),
                }],
                "@solo",
            ),
            (vec![], ""),
        ];
        for (segments, expected) in cases {
            let row = make_row(segments);
            assert_eq!(row.display_text(), expected);
        }
    }

    #[test]
    fn segment_text_roundtrips_via_serde() {
        let json = r#"{"type":"text","text":"hi"}"#;
        let seg: ChatSegment = from_str(json).unwrap();
        assert_eq!(
            seg,
            ChatSegment::Text {
                text: "hi".to_string()
            }
        );
        assert_eq!(to_string(&seg).unwrap(), json);
    }

    #[test]
    fn segment_emote_roundtrips() {
        let json = r#"{"type":"emote","id":"123","name":"KEKW"}"#;
        let seg: ChatSegment = from_str(json).unwrap();
        assert_eq!(
            seg,
            ChatSegment::Emote {
                id: "123".to_string(),
                name: "KEKW".to_string(),
            }
        );
        assert_eq!(to_string(&seg).unwrap(), json);
    }

    #[test]
    fn badge_subscriber_roundtrips() {
        let json = r#"{"kind":"subscriber","months":12}"#;
        let badge: UserBadge = from_str(json).unwrap();
        assert_eq!(badge, UserBadge::Subscriber { months: 12 });
        assert_eq!(to_string(&badge).unwrap(), json);
    }

    #[test]
    fn event_detail_super_chat_roundtrips() {
        let json =
            r#"{"kind":"super_chat","amount_micros":5000000,"currency":"USD","message":"thx"}"#;
        let detail: ChatEventDetail = from_str(json).unwrap();
        assert_eq!(
            detail,
            ChatEventDetail::SuperChat {
                amount_micros: 5_000_000,
                currency: "USD".to_string(),
                message: Some("thx".to_string()),
            }
        );
        assert_eq!(to_string(&detail).unwrap(), json);
    }

    #[test]
    fn moderation_action_serde_tag_and_envelope_roundtrip() {
        let cases = [
            (
                ChatModerationAction::DeleteMessage {
                    message_id: "m1".to_string(),
                },
                "delete_message",
            ),
            (
                ChatModerationAction::RemoveUser {
                    user_name: "bob".to_string(),
                    timeout: true,
                },
                "remove_user",
            ),
            (
                ChatModerationAction::RemoveUser {
                    user_name: "eve".to_string(),
                    timeout: false,
                },
                "remove_user",
            ),
            (ChatModerationAction::ClearChat, "clear_chat"),
        ];

        for (action, tag) in cases {
            let payload = ChatModerationPayload {
                action: action.clone(),
            };
            let envelope = serde_json::json!({ ChatModerationPayload::KEY: payload });
            let stored = &envelope[ChatModerationPayload::KEY];

            assert_eq!(stored["action"]["type"], tag, "wire tag for {action:?}");

            let back: ChatModerationPayload = serde_json::from_value(stored.clone()).unwrap();
            assert_eq!(back.action, action, "round-trip for {action:?}");
        }
    }

    #[test]
    fn chat_source_serde_snake_case() {
        assert_eq!(to_string(&ChatSource::Twitch).unwrap(), r#""twitch""#);
        assert_eq!(to_string(&ChatSource::YouTube).unwrap(), r#""youtube""#);
        assert_eq!(to_string(&ChatSource::Kick).unwrap(), r#""kick""#);
        let src: ChatSource = from_str(r#""twitch""#).unwrap();
        assert_eq!(src, ChatSource::Twitch);
        let src: ChatSource = from_str(r#""youtube""#).unwrap();
        assert_eq!(src, ChatSource::YouTube);
    }
}
