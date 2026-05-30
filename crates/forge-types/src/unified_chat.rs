use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::EventId;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ChatSource {
    Twitch,
    YouTube,
    Kick,
    Trovo,
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
    /// Concatenates only `Text` segments; used for case-insensitive search filtering.
    pub fn body_text(&self) -> String {
        self.body_segments
            .iter()
            .filter_map(|s| match s {
                ChatSegment::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
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
    fn body_text_concats_text_segments_only() {
        let row = make_row(vec![
            ChatSegment::Text {
                text: "hello ".to_string(),
            },
            ChatSegment::Emote {
                id: "1".to_string(),
                name: "KEKW".to_string(),
            },
            ChatSegment::Text {
                text: "world".to_string(),
            },
            ChatSegment::Mention {
                username: "foo".to_string(),
            },
        ]);
        assert_eq!(row.body_text(), "hello world");
    }

    #[test]
    fn body_text_empty_when_no_text_segments() {
        let row = make_row(vec![
            ChatSegment::Emote {
                id: "1".to_string(),
                name: "PogChamp".to_string(),
            },
            ChatSegment::Mention {
                username: "bar".to_string(),
            },
        ]);
        assert_eq!(row.body_text(), "");
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
    fn chat_source_serde_snake_case() {
        assert_eq!(to_string(&ChatSource::Twitch).unwrap(), r#""twitch""#);
        assert_eq!(to_string(&ChatSource::YouTube).unwrap(), r#""youtube""#);
        assert_eq!(to_string(&ChatSource::Kick).unwrap(), r#""kick""#);
        assert_eq!(to_string(&ChatSource::Trovo).unwrap(), r#""trovo""#);
        let src: ChatSource = from_str(r#""twitch""#).unwrap();
        assert_eq!(src, ChatSource::Twitch);
        let src: ChatSource = from_str(r#""youtube""#).unwrap();
        assert_eq!(src, ChatSource::YouTube);
    }

    #[test]
    fn moderation_marks_default_all_false() {
        let marks = ModerationMarks::default();
        assert!(!marks.deleted);
        assert!(!marks.timed_out);
        assert!(!marks.banned);
    }
}
