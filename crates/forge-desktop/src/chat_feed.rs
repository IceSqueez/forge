use std::collections::{BTreeMap, HashMap, VecDeque};

use forge_components::{BadgeKind, ChatBody, Platform, tr};
use forge_events::{Event, EventSource};
use forge_types::{
    ChatEventDetail, ChatModerationAction, ChatModerationPayload, ChatPayload, ChatReply,
    ChatSource, EventId, UnifiedChatRow, UserBadge,
};
use gpui::{Rgba, SharedString};
use time::OffsetDateTime;

pub use forge_storage::DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT as DEFAULT_DISPLAY_LIMIT;

const COMMAND_MATCHED_KIND: &str = "command.matched";
const ACTION_START_KIND: &str = "action.start";

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
    pub reply: Option<(SharedString, SharedString)>,
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
            reply: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AuthorActivity {
    pub message_count: usize,
    pub last_seq: u64,
    pub role: Option<BadgeKind>,
    pub last_received_at: OffsetDateTime,
}

#[derive(Default)]
pub struct AuthorIndex {
    by_name: HashMap<SharedString, AuthorActivity>,
    by_recency: BTreeMap<u64, SharedString>,
}

impl AuthorIndex {
    pub fn get(&self, username: &str) -> Option<&AuthorActivity> {
        self.by_name.get(username)
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &SharedString> {
        self.by_recency.values().rev()
    }

    pub fn newest(&self) -> Option<&SharedString> {
        self.by_recency.values().next_back()
    }

    fn record(&mut self, seq: u64, message: &ChatMessage) {
        if message.username.is_empty() {
            return;
        }
        let role = message.badges.first().copied();
        match self.by_name.get_mut(&message.username) {
            Some(activity) => {
                self.by_recency.remove(&activity.last_seq);
                activity.message_count += 1;
                activity.last_seq = seq;
                activity.role = role;
                activity.last_received_at = message.received_at;
            }
            None => {
                self.by_name.insert(
                    message.username.clone(),
                    AuthorActivity {
                        message_count: 1,
                        last_seq: seq,
                        role,
                        last_received_at: message.received_at,
                    },
                );
            }
        }
        self.by_recency.insert(seq, message.username.clone());
    }

    /// Only valid for the oldest retained row, so the author's newest row is never the one removed while others remain.
    fn forget_oldest(&mut self, message: &ChatMessage) {
        let Some(activity) = self.by_name.get_mut(&message.username) else {
            return;
        };
        activity.message_count = activity.message_count.saturating_sub(1);
        if activity.message_count == 0 {
            self.by_recency.remove(&activity.last_seq);
            self.by_name.remove(&message.username);
        }
    }
}

/// `events` are observer-lag skips whose kinds are unknown, so how many of them were chat messages is not known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeedGap {
    pub messages: u64,
    pub events: u64,
}

impl FeedGap {
    pub fn is_empty(&self) -> bool {
        self.messages == 0 && self.events == 0
    }

    pub fn absorb(&mut self, other: FeedGap) {
        self.messages = self.messages.saturating_add(other.messages);
        self.events = self.events.saturating_add(other.events);
    }

    pub fn label(&self) -> String {
        let messages = i64::try_from(self.messages).unwrap_or(i64::MAX);
        let events = i64::try_from(self.events).unwrap_or(i64::MAX);
        match (self.messages, self.events) {
            (_, 0) => tr!("chat_gap_messages", count = messages),
            (0, _) => tr!("chat_gap_events", count = events),
            _ => tr!("chat_gap_both", messages = messages, events = events),
        }
    }
}

/// Rows carry a monotonic sequence number that survives eviction: `start_seq()` is the oldest retained row.
pub struct ChatFeed {
    messages: VecDeque<ChatMessage>,
    capacity: usize,
    start_seq: u64,
    authors: AuthorIndex,
    /// Keyed by the sequence number of the first row that arrived after the gap.
    gaps: BTreeMap<u64, FeedGap>,
}

impl ChatFeed {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            capacity: DEFAULT_DISPLAY_LIMIT as usize,
            start_seq: 0,
            authors: AuthorIndex::default(),
            gaps: BTreeMap::new(),
        }
    }

    pub fn messages(&self) -> &VecDeque<ChatMessage> {
        &self.messages
    }

    pub fn authors(&self) -> &AuthorIndex {
        &self.authors
    }

    pub fn start_seq(&self) -> u64 {
        self.start_seq
    }

    pub fn end_seq(&self) -> u64 {
        self.start_seq + self.messages.len() as u64
    }

    pub fn get(&self, seq: u64) -> Option<&ChatMessage> {
        let offset = usize::try_from(seq.checked_sub(self.start_seq)?).ok()?;
        self.messages.get(offset)
    }

    /// Clamped to at least one row.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
        self.evict_overflow();
    }

    pub fn push(&mut self, message: ChatMessage) {
        let seq = self.end_seq();
        self.authors.record(seq, &message);
        self.messages.push_back(message);
        self.evict_overflow();
    }

    /// Live rows already present are re-sequenced after the history, so observers see them as evicted and re-appended.
    pub fn seed(&mut self, history: Vec<ChatMessage>) {
        let live = std::mem::take(&mut self.messages);
        let shift = (live.len() + history.len()) as u64;
        let gaps = std::mem::take(&mut self.gaps);
        self.start_seq += live.len() as u64;
        self.authors = AuthorIndex::default();
        for message in history.into_iter().chain(live) {
            self.push(message);
        }
        self.gaps = gaps
            .into_iter()
            .map(|(seq, gap)| (seq + shift, gap))
            .filter(|(seq, _)| *seq >= self.start_seq)
            .collect();
    }

    fn evict_overflow(&mut self) {
        let mut evicted = false;
        while self.messages.len() > self.capacity {
            let Some(oldest) = self.messages.pop_front() else {
                break;
            };
            self.authors.forget_oldest(&oldest);
            self.start_seq += 1;
            evicted = true;
        }
        if evicted {
            self.gaps = self.gaps.split_off(&self.start_seq);
        }
    }

    pub fn record_gap(&mut self, gap: FeedGap) -> bool {
        if gap.is_empty() {
            return false;
        }
        let at = self.end_seq();
        self.gaps.entry(at).or_default().absorb(gap);
        true
    }

    /// Sums every gap recorded after row `after` (the previous shown row, if any) up to and including row `seq`.
    pub fn gap_before(&self, after: Option<u64>, seq: u64) -> FeedGap {
        let from = after.map_or(self.start_seq, |after| after.saturating_add(1));
        let mut total = FeedGap::default();
        if from > seq {
            return total;
        }
        for gap in self.gaps.range(from..=seq).map(|(_, gap)| gap) {
            total.absorb(*gap);
        }
        total
    }

    pub fn apply_event(&mut self, event: &Event) -> bool {
        if let Some(message) = Self::message_from_event(event) {
            self.push(message);
            return true;
        }
        if let Some(value) = event.payload.get(ChatModerationPayload::KEY)
            && let Ok(payload) = serde_json::from_value::<ChatModerationPayload>(value.clone())
            && let Some(platform) = chat_source(event.source).map(platform_of)
        {
            return match payload.action {
                ChatModerationAction::DeleteMessage { message_id } => {
                    self.mark_deleted(&message_id)
                }
                ChatModerationAction::RemoveUser { user_name, .. } => {
                    self.mark_user(platform, &user_name)
                }
                ChatModerationAction::ClearChat => self.clear_platform(platform),
            };
        }
        let Some(caused_by) = event.caused_by else {
            return false;
        };
        match event.kind.as_str() {
            COMMAND_MATCHED_KIND => event
                .payload
                .get("command")
                .and_then(|v| v.as_str())
                .is_some_and(|command| self.mark_command(caused_by, command)),
            ACTION_START_KIND => event
                .payload
                .get("action_name")
                .and_then(|v| v.as_str())
                .is_some_and(|action_name| self.set_triggered(caused_by, action_name)),
            _ => false,
        }
    }

    pub fn annotates_rows(event: &Event) -> bool {
        let moderation = chat_source(event.source).is_some()
            && event.payload.get(ChatModerationPayload::KEY).is_some();
        let run_mark = event.caused_by.is_some()
            && matches!(
                event.kind.as_str(),
                COMMAND_MATCHED_KIND | ACTION_START_KIND
            );
        moderation || run_mark
    }

    pub fn is_message(event: &Event) -> bool {
        chat_source(event.source).is_some() && event.payload.get(ChatPayload::KEY).is_some()
    }

    pub fn set_triggered(&mut self, event_id: EventId, action_name: &str) -> bool {
        let Some(message) = self
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.event_id == event_id)
        else {
            return false;
        };
        match &mut message.body {
            ChatBody::Subscription { triggered, .. }
            | ChatBody::Raid { triggered, .. }
            | ChatBody::Command { triggered, .. } => {
                *triggered = Some(action_name.into());
                true
            }
            ChatBody::Message(_) | ChatBody::Cheer { .. } => false,
        }
    }

    pub fn mark_command(&mut self, event_id: EventId, command: &str) -> bool {
        let Some(message) = self
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.event_id == event_id)
        else {
            return false;
        };
        let triggered = match &message.body {
            ChatBody::Command { triggered, .. } => triggered.clone(),
            _ => None,
        };
        message.body = ChatBody::Command {
            command: command.into(),
            triggered,
        };
        true
    }

    pub fn mark_deleted(&mut self, msg_id: &str) -> bool {
        self.moderate_where(|m| m.id == msg_id)
    }

    pub fn mark_user(&mut self, platform: Platform, username: &str) -> bool {
        self.moderate_where(|m| m.platform == platform && m.username.eq_ignore_ascii_case(username))
    }

    pub fn clear_platform(&mut self, platform: Platform) -> bool {
        self.moderate_where(|m| m.platform == platform)
    }

    fn moderate_where(&mut self, matches: impl Fn(&ChatMessage) -> bool) -> bool {
        let mut changed = false;
        for message in &mut self.messages {
            if !message.moderated && matches(message) {
                message.moderated = true;
                changed = true;
            }
        }
        changed
    }

    pub fn message_from_event(event: &Event) -> Option<ChatMessage> {
        let source = chat_source(event.source)?;
        let chat_value = event.payload.get(ChatPayload::KEY)?;
        let payload: ChatPayload = serde_json::from_value(chat_value.clone()).ok()?;
        let row = row_from_payload(source, event, payload);
        let mut message = ChatMessage::from_row(&row);
        if let Some(reply_value) = event.payload.get(ChatReply::KEY)
            && let Ok(reply) = serde_json::from_value::<ChatReply>(reply_value.clone())
        {
            message.reply = Some((reply.parent_author.into(), reply.parent_text.into()));
        }
        Some(message)
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
        Some(ChatEventDetail::Cheer { bits, message }) => ChatBody::Cheer {
            descriptor: descriptor(tr!("chat_event_cheered")),
            bits: *bits,
            text: message.clone().map(SharedString::from).unwrap_or_default(),
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
    use forge_events::{Event, EventSource};
    use forge_types::{
        ChatEventDetail, ChatModerationAction, ChatModerationPayload, ChatPayload, ChatReply,
        ChatSegment, ChatSource, EventId, ModerationMarks, UnifiedChatRow, UserBadge,
    };
    use time::OffsetDateTime;

    use super::{ChatFeed, ChatMessage, FeedGap, badge_kind, event_body};

    fn message_with(event_id: EventId, body: ChatBody) -> ChatMessage {
        ChatMessage {
            id: "id".into(),
            event_id,
            timestamp: "00:00:00".into(),
            received_at: OffsetDateTime::from_unix_timestamp(0).unwrap(),
            platform: Platform::Twitch,
            badges: vec![],
            username: "user".into(),
            author_color: None,
            body,
            is_event: false,
            is_bot: false,
            moderated: false,
            reply: None,
        }
    }

    fn chat_event(reply: Option<serde_json::Value>) -> Event {
        let payload = ChatPayload {
            platform_msg_id: "m1".to_string(),
            author: "bob".to_string(),
            author_color: None,
            segments: vec![ChatSegment::Text {
                text: "hi".to_string(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        };
        let mut envelope = serde_json::json!({ ChatPayload::KEY: payload });
        if let Some(value) = reply {
            envelope[ChatReply::KEY] = value;
        }
        Event::new(EventSource::Twitch, "chat.message", envelope)
    }

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

        let cheer = row(
            Some(ChatEventDetail::Cheer {
                bits: 250,
                message: Some("pog".to_string()),
            }),
            vec![],
            vec![],
        );
        match event_body(&cheer) {
            ChatBody::Cheer { bits, text, .. } => {
                assert_eq!(bits, 250);
                assert_eq!(text, "pog");
            }
            _ => panic!("Cheer must map to ChatBody::Cheer"),
        }

        let cheer_no_message = row(
            Some(ChatEventDetail::Cheer {
                bits: 1,
                message: None,
            }),
            vec![],
            vec![],
        );
        match event_body(&cheer_no_message) {
            ChatBody::Cheer { bits, text, .. } => {
                assert_eq!(bits, 1);
                assert_eq!(text, "", "absent cheer message maps to empty text");
            }
            _ => panic!("Cheer without a message must still map to ChatBody::Cheer"),
        }

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

    #[test]
    fn mark_command_rewrites_only_the_matching_event_id_to_a_fresh_command() {
        let target = EventId::new();
        let other = EventId::new();
        let mut feed = ChatFeed::new();
        feed.push(message_with(target, ChatBody::Message("hi".into())));
        feed.push(message_with(other, ChatBody::Message("hi".into())));

        feed.mark_command(target, "!lurk");

        match &feed.messages()[0].body {
            ChatBody::Command { command, triggered } => {
                assert_eq!(command, "!lurk");
                assert_eq!(*triggered, None);
            }
            other => panic!("target must become a Command row, got {other:?}"),
        }
        assert!(
            matches!(feed.messages()[1].body, ChatBody::Message(_)),
            "non-matching event_id must be untouched"
        );
    }

    #[test]
    fn mark_command_keeps_existing_trigger_when_body_is_already_a_command() {
        let event_id = EventId::new();
        let mut feed = ChatFeed::new();
        feed.push(message_with(
            event_id,
            ChatBody::Command {
                command: "!old".into(),
                triggered: Some("Greet".into()),
            },
        ));

        feed.mark_command(event_id, "!new");

        match &feed.messages()[0].body {
            ChatBody::Command { command, triggered } => {
                assert_eq!(command, "!new");
                assert_eq!(triggered.as_deref(), Some("Greet"));
            }
            other => panic!("expected Command, got {other:?}"),
        }
    }

    #[test]
    fn set_triggered_fills_command_subscription_and_raid_bodies() {
        let event_id = EventId::new();
        let bodies = [
            ChatBody::Command {
                command: "!x".into(),
                triggered: None,
            },
            ChatBody::Subscription {
                descriptor: "sub".into(),
                months: None,
                message: None,
                triggered: None,
            },
            ChatBody::Raid {
                descriptor: "raid".into(),
                viewers: "5".into(),
                triggered: None,
            },
        ];
        for body in bodies {
            let mut feed = ChatFeed::new();
            feed.push(message_with(event_id, body));
            feed.set_triggered(event_id, "Act");
            let triggered = match &feed.messages()[0].body {
                ChatBody::Command { triggered, .. }
                | ChatBody::Subscription { triggered, .. }
                | ChatBody::Raid { triggered, .. } => triggered.clone(),
                other => panic!("unexpected body {other:?}"),
            };
            assert_eq!(triggered.as_deref(), Some("Act"));
        }
    }

    #[test]
    fn set_triggered_marks_only_the_matching_event_id() {
        let target = EventId::new();
        let other = EventId::new();
        let mut feed = ChatFeed::new();
        feed.push(message_with(
            target,
            ChatBody::Command {
                command: "!x".into(),
                triggered: None,
            },
        ));
        feed.push(message_with(
            other,
            ChatBody::Command {
                command: "!y".into(),
                triggered: None,
            },
        ));

        feed.set_triggered(target, "Act");

        let marked = match &feed.messages()[0].body {
            ChatBody::Command { triggered, .. } => triggered.clone(),
            other => panic!("{other:?}"),
        };
        let untouched = match &feed.messages()[1].body {
            ChatBody::Command { triggered, .. } => triggered.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(marked.as_deref(), Some("Act"));
        assert_eq!(untouched, None);
    }

    #[test]
    fn message_from_event_populates_reply_only_from_a_valid_chat_reply() {
        let with_reply = chat_event(Some(serde_json::json!({
            "parent_author": "alice",
            "parent_text": "hello there",
        })));
        let message = ChatFeed::message_from_event(&with_reply).unwrap();
        assert_eq!(message.reply, Some(("alice".into(), "hello there".into())));

        let absent_or_malformed = [
            None,
            Some(serde_json::json!("not-an-object")),
            Some(serde_json::json!({ "parent_author": "alice" })),
        ];
        for reply in absent_or_malformed {
            let event = chat_event(reply);
            let message = ChatFeed::message_from_event(&event).unwrap();
            assert_eq!(message.reply, None);
        }
    }

    fn authored(id: &str, username: &str, badges: Vec<BadgeKind>, at: i64) -> ChatMessage {
        ChatMessage {
            id: id.to_owned().into(),
            username: username.to_owned().into(),
            badges,
            received_at: OffsetDateTime::from_unix_timestamp(at).unwrap(),
            ..message_with(EventId::new(), ChatBody::Message("hi".into()))
        }
    }

    fn feed_with_capacity(capacity: usize) -> ChatFeed {
        let mut feed = ChatFeed::new();
        feed.set_capacity(capacity);
        feed
    }

    fn retained_ids(feed: &ChatFeed) -> Vec<String> {
        (feed.start_seq()..feed.end_seq())
            .map(|seq| feed.get(seq).unwrap().id.to_string())
            .collect()
    }

    fn ids(range: std::ops::Range<usize>) -> Vec<String> {
        range.map(|i| format!("m{i}")).collect()
    }

    #[test]
    fn pushes_around_the_cap_keep_exactly_the_newest_rows_under_contiguous_seqs() {
        const CAP: usize = 4;
        for pushes in [CAP - 1, CAP, CAP + 1, CAP + 7] {
            let mut feed = feed_with_capacity(CAP);
            for i in 0..pushes {
                feed.push(authored(&format!("m{i}"), "a", vec![], 0));
            }

            let kept = pushes.min(CAP);
            let first = pushes - kept;
            assert_eq!(feed.messages().len(), kept, "pushes={pushes}");
            assert_eq!(feed.start_seq(), first as u64, "pushes={pushes}");
            assert_eq!(feed.end_seq(), pushes as u64, "pushes={pushes}");
            assert_eq!(retained_ids(&feed), ids(first..pushes), "pushes={pushes}");
            assert!(
                first == 0 || feed.get(first as u64 - 1).is_none(),
                "an evicted seq must no longer resolve (pushes={pushes})"
            );
            assert!(feed.get(pushes as u64).is_none(), "pushes={pushes}");
        }
    }

    #[test]
    fn shrinking_the_capacity_evicts_the_oldest_rows_and_zero_still_keeps_one() {
        let mut feed = feed_with_capacity(10);
        for i in 0..6 {
            feed.push(authored(&format!("m{i}"), "a", vec![], 0));
        }

        feed.set_capacity(2);
        assert_eq!(retained_ids(&feed), ids(4..6));

        feed.set_capacity(0);
        assert_eq!(retained_ids(&feed), ids(5..6));
        assert_eq!(feed.end_seq(), 6);
    }

    #[test]
    fn seed_puts_history_ahead_of_live_rows_under_fresh_seqs() {
        let mut feed = feed_with_capacity(10);
        feed.push(authored("live0", "a", vec![], 0));
        feed.push(authored("live1", "b", vec![], 0));
        let end_before = feed.end_seq();

        feed.seed(vec![
            authored("hist0", "c", vec![], 0),
            authored("hist1", "a", vec![], 0),
        ]);

        assert_eq!(retained_ids(&feed), ["hist0", "hist1", "live0", "live1"]);
        assert!(
            feed.start_seq() >= end_before,
            "live rows must be re-sequenced, not keep their old seqs (start {} < {end_before})",
            feed.start_seq()
        );
    }

    #[test]
    fn seed_beyond_the_cap_keeps_the_live_rows_over_the_oldest_history() {
        let mut feed = feed_with_capacity(3);
        feed.push(authored("live0", "a", vec![], 0));

        feed.seed(vec![
            authored("hist0", "a", vec![], 0),
            authored("hist1", "a", vec![], 0),
            authored("hist2", "a", vec![], 0),
        ]);

        assert_eq!(retained_ids(&feed), ["hist1", "hist2", "live0"]);
    }

    #[test]
    fn event_id_mutators_report_a_change_only_when_a_retained_row_changed() {
        let command = EventId::new();
        let plain = EventId::new();
        let evicted = EventId::new();
        let unknown = EventId::new();
        let command_body = || ChatBody::Command {
            command: "!x".into(),
            triggered: None,
        };

        let mut feed = feed_with_capacity(2);
        feed.push(message_with(evicted, command_body()));
        feed.push(message_with(command, command_body()));
        feed.push(message_with(plain, ChatBody::Message("hi".into())));

        assert!(!feed.set_triggered(unknown, "Act"), "no row has this id");
        assert!(!feed.set_triggered(evicted, "Act"), "row was evicted");
        assert!(
            !feed.set_triggered(plain, "Act"),
            "a plain message has no trigger slot"
        );
        assert!(feed.set_triggered(command, "Act"));

        assert!(!feed.mark_command(unknown, "!y"), "no row has this id");
        assert!(!feed.mark_command(evicted, "!y"), "row was evicted");
        assert!(feed.mark_command(plain, "!y"));
    }

    #[test]
    fn moderation_mutators_report_a_change_only_when_a_row_is_newly_moderated() {
        let mut feed = feed_with_capacity(10);
        feed.push(ChatMessage::from_row(&feed_row(
            "tw1",
            ChatSource::Twitch,
            "alice",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "tw2",
            ChatSource::Twitch,
            "bob",
        )));
        feed.push(ChatMessage::from_row(&feed_row(
            "yt1",
            ChatSource::YouTube,
            "carol",
        )));

        assert!(!feed.mark_deleted("ghost"), "no row has this id");
        assert!(feed.mark_deleted("tw1"));
        assert!(!feed.mark_deleted("tw1"), "already moderated");

        assert!(
            !feed.mark_user(Platform::YouTube, "bob"),
            "bob is on Twitch"
        );
        assert!(feed.mark_user(Platform::Twitch, "BOB"));
        assert!(
            !feed.mark_user(Platform::Twitch, "bob"),
            "already moderated"
        );

        assert!(!feed.clear_platform(Platform::Kick), "no Kick rows");
        assert!(
            !feed.clear_platform(Platform::Twitch),
            "every Twitch row is already moderated"
        );
        assert!(feed.clear_platform(Platform::YouTube));
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    type Recomputed = (String, usize, u64, Option<BadgeKind>, OffsetDateTime);

    fn recomputed_authors(feed: &ChatFeed) -> Vec<Recomputed> {
        let mut by_name: std::collections::HashMap<String, Recomputed> =
            std::collections::HashMap::new();
        for (offset, message) in feed.messages().iter().enumerate() {
            if message.username.is_empty() {
                continue;
            }
            let seq = feed.start_seq() + offset as u64;
            let entry = by_name
                .entry(message.username.to_string())
                .or_insert_with(|| {
                    (
                        message.username.to_string(),
                        0,
                        0,
                        None,
                        message.received_at,
                    )
                });
            entry.1 += 1;
            entry.2 = seq;
            entry.3 = message.badges.first().copied();
            entry.4 = message.received_at;
        }
        let mut authors: Vec<Recomputed> = by_name.into_values().collect();
        authors.sort_by_key(|a| std::cmp::Reverse(a.2));
        authors
    }

    fn indexed_authors(feed: &ChatFeed) -> Vec<Recomputed> {
        feed.authors()
            .newest_first()
            .map(|name| {
                let activity = feed.authors().get(name).unwrap();
                (
                    name.to_string(),
                    activity.message_count,
                    activity.last_seq,
                    activity.role,
                    activity.last_received_at,
                )
            })
            .collect()
    }

    #[test]
    fn author_index_matches_a_from_scratch_scan_after_any_push_evict_seed_sequence() {
        const NAMES: [&str; 6] = ["alice", "bob", "carol", "dave", "Alice", ""];
        const BADGES: [BadgeKind; 3] =
            [BadgeKind::Moderator, BadgeKind::Vip, BadgeKind::Subscriber];
        for seed in 1..=24_u64 {
            let mut rng = XorShift(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut feed = feed_with_capacity(1 + rng.below(6) as usize);
            let mut at = 0_i64;
            let mut next_message = |rng: &mut XorShift| {
                at += 1;
                let name = NAMES[rng.below(NAMES.len() as u64) as usize];
                let badges = match rng.below(4) {
                    0 => vec![],
                    n => vec![BADGES[n as usize - 1]],
                };
                authored(&format!("m{at}"), name, badges, at)
            };
            for step in 0..300 {
                match rng.below(20) {
                    0 => feed.set_capacity(1 + rng.below(6) as usize),
                    1 => {
                        let history = (0..rng.below(5)).map(|_| next_message(&mut rng)).collect();
                        feed.seed(history);
                    }
                    _ => feed.push(next_message(&mut rng)),
                }
                assert_eq!(
                    indexed_authors(&feed),
                    recomputed_authors(&feed),
                    "seed={seed} step={step}"
                );
                assert_eq!(
                    feed.authors().len(),
                    recomputed_authors(&feed).len(),
                    "seed={seed} step={step}"
                );
                assert_eq!(
                    feed.authors().newest().map(ToString::to_string),
                    recomputed_authors(&feed).first().map(|a| a.0.clone()),
                    "seed={seed} step={step}"
                );
            }
        }
    }

    fn skipped(messages: u64, events: u64) -> FeedGap {
        FeedGap { messages, events }
    }

    fn pushed(feed: &mut ChatFeed, id: &str) -> u64 {
        let seq = feed.end_seq();
        feed.push(authored(id, "a", vec![], 0));
        seq
    }

    #[test]
    fn a_feed_with_no_recorded_gap_shows_no_gap_before_any_row() {
        let mut feed = feed_with_capacity(10);
        let first = pushed(&mut feed, "m0");
        let second = pushed(&mut feed, "m1");

        assert!(feed.gap_before(None, first).is_empty());
        assert!(feed.gap_before(Some(first), second).is_empty());
    }

    #[test]
    fn recording_an_empty_gap_reports_no_change() {
        let mut feed = feed_with_capacity(10);
        pushed(&mut feed, "m0");

        assert!(!feed.record_gap(FeedGap::default()));
    }

    #[test]
    fn a_gap_is_shown_before_the_first_row_that_arrived_after_it_and_nowhere_else() {
        let mut feed = feed_with_capacity(10);
        let before = pushed(&mut feed, "m0");
        feed.record_gap(skipped(3, 0));
        let after = pushed(&mut feed, "m1");
        let later = pushed(&mut feed, "m2");

        assert_eq!(
            (
                feed.gap_before(None, before),
                feed.gap_before(Some(before), after),
                feed.gap_before(Some(after), later),
            ),
            (FeedGap::default(), skipped(3, 0), FeedGap::default())
        );
    }

    #[test]
    fn a_gap_recorded_before_the_next_row_arrives_is_not_shown_yet() {
        let mut feed = feed_with_capacity(10);
        let last = pushed(&mut feed, "m0");
        feed.record_gap(skipped(2, 0));

        assert!(feed.gap_before(None, last).is_empty());
    }

    #[test]
    fn gaps_with_no_row_between_them_show_as_one_combined_gap() {
        let mut feed = feed_with_capacity(10);
        let before = pushed(&mut feed, "m0");
        feed.record_gap(skipped(2, 0));
        feed.record_gap(skipped(1, 4));
        let after = pushed(&mut feed, "m1");

        assert_eq!(feed.gap_before(Some(before), after), skipped(3, 4));
    }

    #[test]
    fn gaps_separated_by_a_row_stay_separate() {
        let mut feed = feed_with_capacity(10);
        let first = pushed(&mut feed, "m0");
        feed.record_gap(skipped(2, 0));
        let second = pushed(&mut feed, "m1");
        feed.record_gap(skipped(5, 0));
        let third = pushed(&mut feed, "m2");

        assert_eq!(
            (
                feed.gap_before(Some(first), second),
                feed.gap_before(Some(second), third)
            ),
            (skipped(2, 0), skipped(5, 0))
        );
    }

    #[test]
    fn a_filtered_view_sums_every_gap_hidden_between_two_shown_rows() {
        let mut feed = feed_with_capacity(10);
        let shown = pushed(&mut feed, "m0");
        feed.record_gap(skipped(1, 0));
        pushed(&mut feed, "hidden");
        feed.record_gap(skipped(2, 0));
        let next_shown = pushed(&mut feed, "m2");

        assert_eq!(feed.gap_before(Some(shown), next_shown), skipped(3, 0));
    }

    #[test]
    fn eviction_keeps_the_gap_in_front_of_the_oldest_retained_row() {
        let mut feed = feed_with_capacity(2);
        pushed(&mut feed, "m0");
        feed.record_gap(skipped(4, 0));
        pushed(&mut feed, "m1");
        pushed(&mut feed, "m2");

        assert_eq!(feed.gap_before(None, feed.start_seq()), skipped(4, 0));
    }

    #[test]
    fn seeding_history_keeps_each_live_gap_in_front_of_the_same_live_row() {
        let mut feed = feed_with_capacity(10);
        pushed(&mut feed, "live0");
        feed.record_gap(skipped(4, 0));
        pushed(&mut feed, "live1");
        feed.record_gap(skipped(0, 6));

        feed.seed(vec![
            authored("hist0", "c", vec![], 0),
            authored("hist1", "c", vec![], 0),
        ]);
        let live2 = pushed(&mut feed, "live2");

        let seq_of = |id: &str| {
            (feed.start_seq()..feed.end_seq())
                .find(|seq| feed.get(*seq).unwrap().id == id)
                .unwrap()
        };
        let (live0, live1) = (seq_of("live0"), seq_of("live1"));
        assert_eq!(
            (
                feed.gap_before(None, live0),
                feed.gap_before(Some(live0), live1),
                feed.gap_before(Some(live1), live2),
            ),
            (FeedGap::default(), skipped(4, 0), skipped(0, 6))
        );
    }

    #[test]
    fn a_gap_label_names_exact_counts_and_says_when_skipped_events_were_of_unknown_kind() {
        crate::i18n::install_language(forge_storage::Language::En);
        let plain = |gap: FeedGap| gap.label().replace(['\u{2068}', '\u{2069}'], "");

        assert_eq!(
            [
                plain(skipped(1, 0)),
                plain(skipped(12, 0)),
                plain(skipped(0, 7)),
                plain(skipped(3, 9)),
            ],
            [
                "1 message skipped here",
                "12 messages skipped here",
                "7 events skipped here - some may have been messages",
                "3 messages and 9 other events skipped here",
            ]
        );
    }

    fn delete(message_id: &str) -> Event {
        let payload = ChatModerationPayload {
            action: ChatModerationAction::DeleteMessage {
                message_id: message_id.to_owned(),
            },
        };
        Event::new(
            EventSource::Twitch,
            "chat.moderation",
            serde_json::json!({ ChatModerationPayload::KEY: payload }),
        )
    }

    fn caused(kind: &str, payload: serde_json::Value, parent: EventId) -> Event {
        Event::caused_by(EventSource::Core, kind, payload, parent)
    }

    #[test]
    fn apply_event_reports_a_change_only_for_events_that_change_a_row() {
        let mut feed = ChatFeed::new();
        let message = chat_event(None);
        let message_id = message.id;
        let stranger = EventId::new();
        let action = serde_json::json!({ "action_name": "Greet" });
        let command = serde_json::json!({ "command": "!lurk" });
        let steps = [
            ("chat message", message, true),
            (
                "action.start for an unknown event",
                caused("action.start", action.clone(), stranger),
                false,
            ),
            (
                "command.matched for an unknown event",
                caused("command.matched", command.clone(), stranger),
                false,
            ),
            (
                "command.matched with no cause",
                Event::new(EventSource::Core, "command.matched", command.clone()),
                false,
            ),
            (
                "action.start on a plain message row",
                caused("action.start", action.clone(), message_id),
                false,
            ),
            ("delete of an unknown message", delete("ghost"), false),
            (
                "command.matched on the row",
                caused("command.matched", command, message_id),
                true,
            ),
            (
                "action.start on the command row",
                caused("action.start", action, message_id),
                true,
            ),
            ("delete of the row", delete("m1"), true),
            ("repeated delete of the row", delete("m1"), false),
        ];
        for (label, event, expect_change) in steps {
            assert_eq!(feed.apply_event(&event), expect_change, "{label}");
        }
    }
}
