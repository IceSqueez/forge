use serde::{Deserialize, Serialize};
use std::fmt;
use time::OffsetDateTime;

use crate::EventId;
use crate::redaction::{Redacted, RedactedText};

/// Attached under `Event::payload["_chat"]`; the `_` prefix is reserved for forge-internal keys (ArgStack keys never start with `_`).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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

impl fmt::Debug for ChatPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatPayload")
            .field("platform_msg_id", &self.platform_msg_id)
            .field("author", &Redacted)
            .field("author_color", &self.author_color)
            .field("segments", &self.segments)
            .field("badges", &self.badges)
            .field("is_event", &self.is_event)
            .field("event_detail", &self.event_detail)
            .field("moderation", &self.moderation)
            .finish()
    }
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

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatModerationAction {
    DeleteMessage { message_id: String },
    RemoveUser { user_name: String, timeout: bool },
    ClearChat,
}

impl fmt::Debug for ChatModerationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeleteMessage { message_id } => f
                .debug_struct("DeleteMessage")
                .field("message_id", message_id)
                .finish(),
            Self::RemoveUser { timeout, .. } => f
                .debug_struct("RemoveUser")
                .field("user_name", &Redacted)
                .field("timeout", timeout)
                .finish(),
            Self::ClearChat => f.write_str("ClearChat"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReply {
    pub parent_author: String,
    pub parent_text: String,
}

impl fmt::Debug for ChatReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatReply")
            .field("parent_author", &Redacted)
            .field("parent_text", &RedactedText::new(&self.parent_text))
            .finish()
    }
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

#[derive(Clone, Serialize, Deserialize)]
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

impl fmt::Debug for UnifiedChatRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnifiedChatRow")
            .field("id", &self.id)
            .field("event_id", &self.event_id)
            .field("source", &self.source)
            .field("received_at", &self.received_at)
            .field("author", &Redacted)
            .field("author_color", &self.author_color)
            .field("body_segments", &self.body_segments)
            .field("badges", &self.badges)
            .field("is_event", &self.is_event)
            .field("event_detail", &self.event_detail)
            .field("moderation", &self.moderation)
            .finish()
    }
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

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatSegment {
    Text { text: String },
    Emote { id: String, name: String },
    Link { url: String, display: String },
    Mention { username: String },
}

impl fmt::Debug for ChatSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text { text } => f
                .debug_struct("Text")
                .field("text", &RedactedText::new(text))
                .finish(),
            Self::Emote { id, name } => f
                .debug_struct("Emote")
                .field("id", id)
                .field("name", name)
                .finish(),
            Self::Link { url, display } => f
                .debug_struct("Link")
                .field("url", &RedactedText::new(url))
                .field("display", &RedactedText::new(display))
                .finish(),
            Self::Mention { .. } => f
                .debug_struct("Mention")
                .field("username", &Redacted)
                .finish(),
        }
    }
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

#[derive(Clone, Serialize, Deserialize, PartialEq)]
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

impl fmt::Debug for ChatEventDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted = |message: &Option<String>| message.as_deref().map(RedactedText::new);
        match self {
            Self::Subscription {
                tier,
                months,
                message,
            } => f
                .debug_struct("Subscription")
                .field("tier", tier)
                .field("months", months)
                .field("message", &redacted(message))
                .finish(),
            Self::Raid { viewer_count } => f
                .debug_struct("Raid")
                .field("viewer_count", viewer_count)
                .finish(),
            Self::Cheer { bits, message } => f
                .debug_struct("Cheer")
                .field("bits", bits)
                .field("message", &redacted(message))
                .finish(),
            Self::SuperChat {
                amount_micros,
                currency,
                message,
            } => f
                .debug_struct("SuperChat")
                .field("amount_micros", amount_micros)
                .field("currency", currency)
                .field("message", &redacted(message))
                .finish(),
            Self::NewMember { level } => f.debug_struct("NewMember").field("level", level).finish(),
            Self::MemberMilestone { months, message } => f
                .debug_struct("MemberMilestone")
                .field("months", months)
                .field("message", &redacted(message))
                .finish(),
        }
    }
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

    fn full_payload() -> ChatPayload {
        ChatPayload {
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
        }
    }

    #[test]
    fn chat_payload_roundtrips_from_empty_to_fully_populated() {
        for payload in [minimal_payload("abc123"), full_payload()] {
            let json = to_string(&payload).unwrap();
            let back: ChatPayload = from_str(&json).unwrap();
            assert_eq!(back, payload);
        }
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
    fn segment_wire_format_roundtrips_for_every_arm() {
        let cases = [
            (
                ChatSegment::Text {
                    text: "hi".to_string(),
                },
                r#"{"type":"text","text":"hi"}"#,
            ),
            (
                ChatSegment::Emote {
                    id: "123".to_string(),
                    name: "KEKW".to_string(),
                },
                r#"{"type":"emote","id":"123","name":"KEKW"}"#,
            ),
            (
                ChatSegment::Link {
                    url: "https://example.test/a".to_string(),
                    display: "example.test".to_string(),
                },
                r#"{"type":"link","url":"https://example.test/a","display":"example.test"}"#,
            ),
            (
                ChatSegment::Mention {
                    username: "foo".to_string(),
                },
                r#"{"type":"mention","username":"foo"}"#,
            ),
        ];
        for (segment, json) in cases {
            assert_eq!(to_string(&segment).unwrap(), json);
            assert_eq!(from_str::<ChatSegment>(json).unwrap(), segment);
        }
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

    const AUTHOR: &str = "SENTINEL_AUTHOR";
    const BODY: &str = "SENTINEL_BODY";

    fn payload_with_planted_content() -> ChatPayload {
        ChatPayload {
            platform_msg_id: "msg-42".to_string(),
            author: AUTHOR.to_string(),
            author_color: None,
            segments: vec![
                ChatSegment::Text {
                    text: BODY.to_string(),
                },
                ChatSegment::Emote {
                    id: "emote-7".to_string(),
                    name: "KEKW".to_string(),
                },
            ],
            badges: vec![UserBadge::Moderator],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn row_with_planted_content() -> UnifiedChatRow {
        let mut row = make_row(vec![
            ChatSegment::Text {
                text: BODY.to_string(),
            },
            ChatSegment::Emote {
                id: "emote-7".to_string(),
                name: "KEKW".to_string(),
            },
        ]);
        row.author = AUTHOR.to_string();
        row
    }

    #[test]
    fn chat_payload_debug_hides_the_author_and_its_nested_segment_text() {
        let rendered = format!("{:?}", payload_with_planted_content());
        assert!(!rendered.contains("SENTINEL"), "{rendered}");
        assert!(rendered.contains("author: <redacted>"), "{rendered}");
        assert!(rendered.contains("text: <redacted len=13>"), "{rendered}");
    }

    #[test]
    fn chat_segment_debug_hides_authored_text_and_mentioned_identity() {
        let cases = [
            (
                ChatSegment::Text {
                    text: BODY.to_string(),
                },
                "Text { text: <redacted len=13> }",
            ),
            (
                ChatSegment::Link {
                    url: format!("https://example.test/{BODY}"),
                    display: BODY.to_string(),
                },
                "Link { url: <redacted len=34>, display: <redacted len=13> }",
            ),
            (
                ChatSegment::Mention {
                    username: AUTHOR.to_string(),
                },
                "Mention { username: <redacted> }",
            ),
        ];
        for (segment, expected) in cases {
            assert_eq!(format!("{segment:?}"), expected);
        }
    }

    #[test]
    fn chat_event_detail_debug_hides_the_message_of_every_carrying_variant() {
        let cases = [
            (
                ChatEventDetail::Subscription {
                    tier: 2,
                    months: Some(7),
                    message: Some(BODY.to_string()),
                },
                "Subscription { tier: 2, months: Some(7), message: Some(<redacted len=13>) }",
            ),
            (
                ChatEventDetail::Cheer {
                    bits: 500,
                    message: Some(BODY.to_string()),
                },
                "Cheer { bits: 500, message: Some(<redacted len=13>) }",
            ),
            (
                ChatEventDetail::SuperChat {
                    amount_micros: 5_000_000,
                    currency: "USD".to_string(),
                    message: Some(BODY.to_string()),
                },
                "SuperChat { amount_micros: 5000000, currency: \"USD\", message: Some(<redacted len=13>) }",
            ),
            (
                ChatEventDetail::MemberMilestone {
                    months: 12,
                    message: Some(BODY.to_string()),
                },
                "MemberMilestone { months: 12, message: Some(<redacted len=13>) }",
            ),
        ];
        for (detail, expected) in cases {
            assert_eq!(format!("{detail:?}"), expected);
        }
    }

    #[test]
    fn chat_event_detail_debug_distinguishes_an_absent_message_from_an_empty_one() {
        assert!(
            format!(
                "{:?}",
                ChatEventDetail::Cheer {
                    bits: 1,
                    message: None
                }
            )
            .contains("message: None")
        );
        assert!(
            format!(
                "{:?}",
                ChatEventDetail::Cheer {
                    bits: 1,
                    message: Some(String::new())
                }
            )
            .contains("message: Some(<redacted len=0>)")
        );
    }

    #[test]
    fn chat_reply_debug_hides_the_parent_author_and_parent_text() {
        let reply = ChatReply {
            parent_author: AUTHOR.to_string(),
            parent_text: BODY.to_string(),
        };
        assert_eq!(
            format!("{reply:?}"),
            "ChatReply { parent_author: <redacted>, parent_text: <redacted len=13> }"
        );
    }

    #[test]
    fn unified_chat_row_debug_hides_the_author_and_its_body_text() {
        let rendered = format!("{:?}", row_with_planted_content());
        assert!(!rendered.contains("SENTINEL"), "{rendered}");
        assert!(rendered.contains("author: <redacted>"), "{rendered}");
        assert!(rendered.contains("text: <redacted len=13>"), "{rendered}");
    }

    #[test]
    fn chat_moderation_action_debug_hides_the_removed_user_name() {
        let rendered = format!(
            "{:?}",
            ChatModerationAction::RemoveUser {
                user_name: AUTHOR.to_string(),
                timeout: true,
            }
        );
        assert_eq!(
            rendered,
            "RemoveUser { user_name: <redacted>, timeout: true }"
        );
    }

    /// Why: these are opaque ids and platform-catalog values, not viewer prose - a later, broader
    /// redaction sweep that swallowed them would leave a maintainer unable to correlate a report.
    #[test]
    fn chat_debug_keeps_ids_and_platform_catalog_values_verbatim() {
        let cases: Vec<(&str, String, Vec<&str>)> = vec![
            (
                "ChatPayload",
                format!("{:?}", payload_with_planted_content()),
                vec!["msg-42", "emote-7", "KEKW", "Moderator"],
            ),
            (
                "UnifiedChatRow",
                format!("{:?}", row_with_planted_content()),
                vec!["test-id", "Twitch", "emote-7", "KEKW"],
            ),
            (
                "ChatModerationAction::DeleteMessage",
                format!(
                    "{:?}",
                    ChatModerationAction::DeleteMessage {
                        message_id: "m1".to_string()
                    }
                ),
                vec!["m1"],
            ),
            (
                "ChatEventDetail::Raid",
                format!("{:?}", ChatEventDetail::Raid { viewer_count: 128 }),
                vec!["128"],
            ),
        ];
        for (label, rendered, expected) in cases {
            for fragment in expected {
                assert!(
                    rendered.contains(fragment),
                    "{label} dropped {fragment:?}: {rendered}"
                );
            }
        }
    }

    /// Why: redaction is a `Debug`-only layer - `display_text` feeds the chat view, so narrowing it
    /// would blank every message on screen.
    #[test]
    fn unified_chat_row_display_text_still_exposes_authored_text() {
        let row = row_with_planted_content();
        assert_eq!(row.display_text(), BODY);
        assert!(!format!("{row:?}").contains(BODY));
    }

    #[test]
    fn chat_reply_and_unified_chat_row_survive_a_serde_roundtrip() {
        let reply = ChatReply {
            parent_author: "streamer".to_string(),
            parent_text: "original message".to_string(),
        };
        let back: ChatReply = from_str(&to_string(&reply).unwrap()).unwrap();
        assert_eq!(back, reply);

        let row_json = to_string(&row_with_planted_content()).unwrap();
        let back: UnifiedChatRow = from_str(&row_json).unwrap();
        assert_eq!(to_string(&back).unwrap(), row_json);
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
