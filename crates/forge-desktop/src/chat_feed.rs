use forge_components::{BadgeKind, ChatBody, Platform, tr};
use forge_events::{Event, EventSource};
use forge_types::{ChatEventDetail, ChatPayload, ChatSource, UnifiedChatRow, UserBadge};
use gpui::SharedString;

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub timestamp: SharedString,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: SharedString,
    pub body: ChatBody,
    pub is_event: bool,
    pub is_bot: bool,
}

impl ChatMessage {
    /// `query` must already be lowercased by the caller.
    pub fn matches_query(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        if self.username.to_lowercase().contains(query) {
            return true;
        }
        let body_text = match &self.body {
            ChatBody::Message(text) => text.to_lowercase(),
            ChatBody::Command { command, .. } => command.to_lowercase(),
            ChatBody::Cheer { text, .. } => text.to_lowercase(),
            ChatBody::Subscription {
                descriptor,
                message,
                ..
            } => message
                .as_ref()
                .map_or_else(|| descriptor.to_lowercase(), |m| m.to_lowercase()),
            ChatBody::Raid { descriptor, .. } => descriptor.to_lowercase(),
        };
        body_text.contains(query)
    }

    pub fn from_row(row: &UnifiedChatRow) -> ChatMessage {
        ChatMessage {
            timestamp: format_clock(row.received_at.unix_timestamp()).into(),
            platform: platform_of(row.source),
            badges: row.badges.iter().filter_map(badge_kind).collect(),
            username: row.author.clone().into(),
            body: event_body(row),
            is_event: row.is_event,
            is_bot: row.badges.iter().any(|b| matches!(b, UserBadge::Bot)),
        }
    }
}

pub struct ChatFeed {
    messages: Vec<ChatMessage>,
}

impl ChatFeed {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// In arrival order, oldest first.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Prepends persisted history ahead of anything already appended, keeping the
    /// oldest-first ordering `history` arrives in.
    pub fn seed(&mut self, mut history: Vec<ChatMessage>) {
        if self.messages.is_empty() {
            self.messages = history;
        } else {
            history.append(&mut self.messages);
            self.messages = history;
        }
    }

    pub fn message_from_event(event: &Event) -> Option<ChatMessage> {
        let source = chat_source(event.source)?;
        let chat_value = event.payload.get(ChatPayload::KEY)?;
        let payload: ChatPayload = serde_json::from_value(chat_value.clone()).ok()?;
        let row = row_from_payload(source, event, payload);
        Some(ChatMessage::from_row(&row))
    }
}

fn chat_source(src: EventSource) -> Option<ChatSource> {
    match src {
        EventSource::Twitch => Some(ChatSource::Twitch),
        EventSource::YouTube => Some(ChatSource::YouTube),
        EventSource::Kick => Some(ChatSource::Kick),
        EventSource::Core
        | EventSource::Rhai
        | EventSource::Http
        | EventSource::Obs
        | EventSource::VTube
        | EventSource::Discord
        | EventSource::Midi
        | EventSource::Hotkey
        | EventSource::Timer
        | EventSource::Server
        | EventSource::Audio => None,
    }
}

fn row_from_payload(source: ChatSource, event: &Event, payload: ChatPayload) -> UnifiedChatRow {
    let author_color = payload
        .author_color
        .as_deref()
        .and_then(ChatPayload::parse_color);
    UnifiedChatRow {
        id: payload.platform_msg_id,
        event_id: event.id,
        source,
        received_at: event.timestamp,
        author: payload.author,
        author_color,
        body_segments: payload.segments,
        badges: payload.badges,
        is_event: payload.is_event,
        event_detail: payload.event_detail,
        moderation: payload.moderation,
    }
}

fn platform_of(source: ChatSource) -> Platform {
    match source {
        ChatSource::Twitch => Platform::Twitch,
        ChatSource::YouTube => Platform::YouTube,
        ChatSource::Kick => Platform::Kick,
    }
}

fn badge_kind(badge: &UserBadge) -> Option<BadgeKind> {
    match badge {
        UserBadge::Broadcaster => Some(BadgeKind::Broadcaster),
        UserBadge::Moderator => Some(BadgeKind::Moderator),
        UserBadge::Vip => Some(BadgeKind::Vip),
        UserBadge::Subscriber { .. } => Some(BadgeKind::Subscriber),
        UserBadge::Bot => Some(BadgeKind::Bot),
        UserBadge::Partner => Some(BadgeKind::Partner),
        UserBadge::Premium => Some(BadgeKind::Premium),
        UserBadge::Founder => Some(BadgeKind::Founder),
        UserBadge::Turbo => Some(BadgeKind::Turbo),
        UserBadge::HypeTrain => Some(BadgeKind::HypeTrain),
        UserBadge::Bits { .. } => Some(BadgeKind::Bits),
        UserBadge::BitsLeader { .. } => Some(BadgeKind::BitsLeader),
        UserBadge::Member { .. } => None,
    }
}

fn event_body(row: &UnifiedChatRow) -> ChatBody {
    match &row.event_detail {
        None => ChatBody::Message(row.display_text().into()),
        Some(ChatEventDetail::Subscription {
            tier,
            months,
            message,
        }) => ChatBody::Subscription {
            descriptor: descriptor(tr!("chat_event_subscribed", tier = i64::from(*tier))),
            months: *months,
            message: message.clone().map(SharedString::from),
            triggered: None,
        },
        Some(ChatEventDetail::Raid { viewer_count }) => ChatBody::Raid {
            descriptor: descriptor(tr!("chat_event_raided")),
            viewers: tr!("chat_event_viewers", viewers = *viewer_count as i64).into(),
            triggered: None,
        },
        Some(ChatEventDetail::SuperChat {
            amount_micros,
            currency,
            message,
        }) => ChatBody::Subscription {
            descriptor: descriptor(tr!(
                "chat_event_super_chat",
                amount = format_amount(*amount_micros),
                currency = currency.clone()
            )),
            months: None,
            message: message.clone().map(SharedString::from),
            triggered: None,
        },
        Some(ChatEventDetail::NewMember { .. }) => ChatBody::Subscription {
            descriptor: descriptor(tr!("chat_event_new_member")),
            months: None,
            message: None,
            triggered: None,
        },
        Some(ChatEventDetail::MemberMilestone { months, message }) => ChatBody::Subscription {
            descriptor: descriptor(tr!("chat_event_member_milestone")),
            months: Some(*months),
            message: message.clone().map(SharedString::from),
            triggered: None,
        },
    }
}

/// The kit renders descriptors verbatim next to the username, so it carries a
/// leading space regardless of the active locale's own spacing.
fn descriptor(text: String) -> SharedString {
    format!(" {text}").into()
}

fn format_amount(micros: u64) -> String {
    let whole = micros / 1_000_000;
    let cents = (micros % 1_000_000) / 10_000;
    format!("{whole}.{cents:02}")
}

fn format_clock(unix_secs: i64) -> String {
    let secs = unix_secs.rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_components::{BadgeKind, ChatBody};
    use forge_types::{
        ChatEventDetail, ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow,
        UserBadge,
    };
    use time::OffsetDateTime;

    use super::{ChatMessage, badge_kind, event_body};

    fn row(
        event_detail: Option<ChatEventDetail>,
        segments: Vec<ChatSegment>,
        badges: Vec<UserBadge>,
    ) -> UnifiedChatRow {
        UnifiedChatRow {
            id: "id".to_string(),
            event_id: EventId::new(),
            source: ChatSource::Twitch,
            received_at: OffsetDateTime::from_unix_timestamp(0).unwrap(),
            author: "user".to_string(),
            author_color: None,
            body_segments: segments,
            badges,
            is_event: event_detail.is_some(),
            event_detail,
            moderation: ModerationMarks::default(),
        }
    }

    #[test]
    fn event_body_maps_each_event_detail_variant() {
        // `None` renders the plain message text (Text + Mention segments). Each
        // ChatEventDetail maps to a specific ChatBody; the four kinds that share
        // ChatBody::Subscription are told apart by their `months`/`message`.
        let plain = row(
            None,
            vec![
                ChatSegment::Text {
                    text: "hello ".to_string(),
                },
                ChatSegment::Mention {
                    username: "bob".to_string(),
                },
            ],
            vec![],
        );
        match event_body(&plain) {
            ChatBody::Message(text) => assert_eq!(text, "hello @bob"),
            _ => panic!("None event_detail must map to ChatBody::Message"),
        }

        let subscription = row(
            Some(ChatEventDetail::Subscription {
                tier: 1,
                months: Some(6),
                message: Some("hi".to_string()),
            }),
            vec![],
            vec![],
        );
        match event_body(&subscription) {
            ChatBody::Subscription {
                months, message, ..
            } => {
                assert_eq!(months, Some(6));
                assert_eq!(message, Some("hi".into()));
            }
            _ => panic!("Subscription must map to ChatBody::Subscription"),
        }

        let raid = row(
            Some(ChatEventDetail::Raid { viewer_count: 50 }),
            vec![],
            vec![],
        );
        assert!(
            matches!(event_body(&raid), ChatBody::Raid { .. }),
            "Raid must map to ChatBody::Raid"
        );

        let super_chat = row(
            Some(ChatEventDetail::SuperChat {
                amount_micros: 5_000_000,
                currency: "USD".to_string(),
                message: Some("thx".to_string()),
            }),
            vec![],
            vec![],
        );
        match event_body(&super_chat) {
            ChatBody::Subscription {
                months, message, ..
            } => {
                assert_eq!(months, None);
                assert_eq!(message, Some("thx".into()));
            }
            _ => panic!("SuperChat must map to ChatBody::Subscription"),
        }

        let new_member = row(
            Some(ChatEventDetail::NewMember {
                level: "gold".to_string(),
            }),
            vec![],
            vec![],
        );
        match event_body(&new_member) {
            ChatBody::Subscription {
                months, message, ..
            } => {
                assert_eq!(months, None);
                assert_eq!(message, None);
            }
            _ => panic!("NewMember must map to ChatBody::Subscription"),
        }

        let milestone = row(
            Some(ChatEventDetail::MemberMilestone {
                months: 9,
                message: None,
            }),
            vec![],
            vec![],
        );
        match event_body(&milestone) {
            ChatBody::Subscription { months, .. } => assert_eq!(months, Some(9)),
            _ => panic!("MemberMilestone must map to ChatBody::Subscription"),
        }
    }

    #[test]
    fn badge_kind_maps_every_user_badge_with_member_hidden() {
        let cases = [
            (UserBadge::Broadcaster, Some(BadgeKind::Broadcaster)),
            (UserBadge::Moderator, Some(BadgeKind::Moderator)),
            (UserBadge::Vip, Some(BadgeKind::Vip)),
            (
                UserBadge::Subscriber { months: 3 },
                Some(BadgeKind::Subscriber),
            ),
            (UserBadge::Bot, Some(BadgeKind::Bot)),
            (UserBadge::Partner, Some(BadgeKind::Partner)),
            (UserBadge::Premium, Some(BadgeKind::Premium)),
            (UserBadge::Founder, Some(BadgeKind::Founder)),
            (UserBadge::Turbo, Some(BadgeKind::Turbo)),
            (UserBadge::HypeTrain, Some(BadgeKind::HypeTrain)),
            (UserBadge::Bits { amount: 100 }, Some(BadgeKind::Bits)),
            (
                UserBadge::BitsLeader { rank: 1 },
                Some(BadgeKind::BitsLeader),
            ),
            (
                UserBadge::Member {
                    level: "gold".to_string(),
                },
                None,
            ),
        ];
        for (badge, expected) in cases {
            assert_eq!(
                badge_kind(&badge),
                expected,
                "mapping mismatch for {badge:?}"
            );
        }
    }

    #[test]
    fn is_bot_is_true_only_when_a_bot_badge_is_present() {
        let cases = [
            (vec![UserBadge::Bot], true),
            (vec![UserBadge::Moderator], false),
            (vec![UserBadge::Moderator, UserBadge::Bot], true),
            (vec![], false),
        ];
        for (badges, expected) in cases {
            let message = ChatMessage::from_row(&row(None, vec![], badges.clone()));
            assert_eq!(message.is_bot, expected, "is_bot mismatch for {badges:?}");
        }
    }
}
