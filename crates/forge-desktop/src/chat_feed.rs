use forge_components::{BadgeKind, ChatBody, Platform, tr};
use forge_events::{Event, EventSource};
use forge_types::{ChatEventDetail, ChatPayload, ChatSource, EventId, UnifiedChatRow, UserBadge};
use gpui::{Rgba, SharedString};
use time::OffsetDateTime;

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub id: SharedString,
    pub event_id: EventId,
    pub timestamp: SharedString,
    pub received_at: OffsetDateTime,
    pub platform: Platform,
    pub badges: Vec<BadgeKind>,
    pub username: SharedString,
    pub author_color: Option<Rgba>,
    pub body: ChatBody,
    pub is_event: bool,
    pub is_bot: bool,
    pub moderated: bool,
}

impl ChatMessage {
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
        let moderation = &row.moderation;
        ChatMessage {
            id: row.id.clone().into(),
            event_id: row.event_id,
            timestamp: format_clock(row.received_at.unix_timestamp()).into(),
            received_at: row.received_at,
            platform: platform_of(row.source),
            badges: row.badges.iter().filter_map(badge_kind).collect(),
            username: row.author.clone().into(),
            author_color: row.author_color.map(rgb_channels),
            body: event_body(row),
            is_event: row.is_event,
            is_bot: row.badges.iter().any(|b| matches!(b, UserBadge::Bot)),
            moderated: moderation.deleted || moderation.timed_out || moderation.banned,
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

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub fn seed(&mut self, mut history: Vec<ChatMessage>) {
        if self.messages.is_empty() {
            self.messages = history;
        } else {
            history.append(&mut self.messages);
            self.messages = history;
        }
    }

    pub fn set_triggered(&mut self, event_id: EventId, action_name: &str) {
        for message in &mut self.messages {
            if message.event_id == event_id
                && let ChatBody::Subscription { triggered, .. } | ChatBody::Raid { triggered, .. } =
                    &mut message.body
            {
                *triggered = Some(action_name.into());
            }
        }
    }

    pub fn mark_deleted(&mut self, msg_id: &str) {
        for message in &mut self.messages {
            if message.id == msg_id {
                message.moderated = true;
            }
        }
    }

    pub fn mark_user(&mut self, platform: Platform, username: &str) {
        for message in &mut self.messages {
            if message.platform == platform && message.username.eq_ignore_ascii_case(username) {
                message.moderated = true;
            }
        }
    }

    pub fn clear_platform(&mut self, platform: Platform) {
        for message in &mut self.messages {
            if message.platform == platform {
                message.moderated = true;
            }
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

pub(crate) fn chat_source(src: EventSource) -> Option<ChatSource> {
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

fn rgb_channels([r, g, b]: [u8; 3]) -> Rgba {
    Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: 1.0,
    }
}

pub(crate) fn platform_of(source: ChatSource) -> Platform {
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
    use forge_components::{BadgeKind, ChatBody, Platform};
    use forge_types::{
        ChatEventDetail, ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow,
        UserBadge,
    };
    use time::OffsetDateTime;

    use super::{ChatFeed, ChatMessage, badge_kind, event_body};

    fn feed_row(id: &str, source: ChatSource, author: &str) -> UnifiedChatRow {
        UnifiedChatRow {
            id: id.to_string(),
            source,
            author: author.to_string(),
            ..row(None, vec![], vec![])
        }
    }

    fn moderated_by_id(feed: &ChatFeed, id: &str) -> bool {
        feed.messages()
            .iter()
            .find(|m| m.id == id)
            .unwrap()
            .moderated
    }

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

    #[test]
    fn from_row_moderated_is_true_when_any_moderation_flag_is_set() {
        let cases = [
            (ModerationMarks::default(), false),
            (
                ModerationMarks {
                    deleted: true,
                    ..Default::default()
                },
                true,
            ),
            (
                ModerationMarks {
                    timed_out: true,
                    ..Default::default()
                },
                true,
            ),
            (
                ModerationMarks {
                    banned: true,
                    ..Default::default()
                },
                true,
            ),
        ];
        for (marks, expected) in cases {
            let mut r = row(None, vec![], vec![]);
            r.moderation = marks.clone();
            assert_eq!(
                ChatMessage::from_row(&r).moderated,
                expected,
                "marks={marks:?}"
            );
        }
    }

    #[test]
    fn mark_deleted_flips_only_the_message_with_matching_id() {
        let mut feed = ChatFeed::new();
        feed.push(ChatMessage::from_row(&feed_row(
            "m1",
            ChatSource::Twitch,
            "alice",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "m2",
            ChatSource::Twitch,
            "bob",
        )));

        feed.mark_deleted("m1");

        assert!(moderated_by_id(&feed, "m1"));
        assert!(!moderated_by_id(&feed, "m2"));
    }

    #[test]
    fn mark_user_flips_matching_platform_and_case_insensitive_username_only() {
        let mut feed = ChatFeed::new();
        feed.push(ChatMessage::from_row(&feed_row(
            "same",
            ChatSource::Twitch,
            "Alice",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "other-name",
            ChatSource::Twitch,
            "Bob",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "other-platform",
            ChatSource::YouTube,
            "Alice",
        )));

        feed.mark_user(Platform::Twitch, "alice");

        assert!(
            moderated_by_id(&feed, "same"),
            "same platform, case-insensitive name match"
        );
        assert!(
            !moderated_by_id(&feed, "other-name"),
            "different username untouched"
        );
        assert!(
            !moderated_by_id(&feed, "other-platform"),
            "same name on another platform untouched"
        );
    }

    #[test]
    fn clear_platform_flips_only_messages_on_that_platform() {
        let mut feed = ChatFeed::new();
        feed.push(ChatMessage::from_row(&feed_row(
            "tw",
            ChatSource::Twitch,
            "a",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "yt",
            ChatSource::YouTube,
            "b",
        )));

        feed.clear_platform(Platform::Twitch);

        assert!(moderated_by_id(&feed, "tw"));
        assert!(!moderated_by_id(&feed, "yt"));
    }
}
