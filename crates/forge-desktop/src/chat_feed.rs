use forge_components::{BadgeKind, ChatBody, Platform};
use forge_events::Event;
use gpui::SharedString;

/// One chat line held by the [`ChatFeed`] topic. Carries the source-neutral facts
/// a row needs (author, platform, badges, typed body, timestamp) but NOT any
/// resolved color: the username hue is derived from the active theme at render
/// time so the row re-tints when the palette changes. `is_event` / `is_bot` are
/// precomputed so the screen's filters stay pure and side-effect-free.
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
    fn message(
        timestamp: &'static str,
        platform: Platform,
        badges: Vec<BadgeKind>,
        username: &'static str,
        text: &'static str,
    ) -> Self {
        let is_bot = badges.contains(&BadgeKind::Bot);
        Self {
            timestamp: timestamp.into(),
            platform,
            badges,
            username: username.into(),
            body: ChatBody::Message(text.into()),
            is_event: false,
            is_bot,
        }
    }

    fn event(
        timestamp: &'static str,
        platform: Platform,
        badges: Vec<BadgeKind>,
        username: &'static str,
        body: ChatBody,
    ) -> Self {
        Self {
            timestamp: timestamp.into(),
            platform,
            badges,
            username: username.into(),
            body,
            is_event: true,
            is_bot: false,
        }
    }

    /// True when `query` (already lowercased by the caller) is found in the author
    /// name or the body's primary text. Drives the search dim: a non-match is faded
    /// rather than removed.
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
}

/// Topic-scoped observable entity fed by the runtime→UI bridge: the sole owner of
/// the runtime-chat→UI edge. The bridge drains the event bus and appends each
/// decoded chat line here, then `cx.notify()`s so the observing chat screen
/// repaints. Holds no runtime state of its own — only the rows it has been handed.
///
/// Seeded at boot with a representative sample so the screen renders visibly
/// before any platform connection exists; real events (once platforms publish
/// them) append through the same [`ChatFeed::push`] path and replace the sample as
/// they arrive.
pub struct ChatFeed {
    messages: Vec<ChatMessage>,
}

impl ChatFeed {
    /// A representative starter set: three platforms, role badges (MOD / SUB / VIP
    /// / BOT), and event rows (subscription, raid with a triggered action, cheer).
    /// Clearly a slice stub — real connections drain real events over the bridge.
    pub fn seeded() -> Self {
        let messages = vec![
            ChatMessage::message(
                "14:02:07",
                Platform::Twitch,
                vec![BadgeKind::Moderator],
                "koval_dev",
                "Hello everyone! Stream looks great today",
            ),
            ChatMessage::message(
                "14:02:19",
                Platform::YouTube,
                vec![],
                "olena_lv",
                "First time catching this live, loving the setup",
            ),
            ChatMessage::message(
                "14:02:34",
                Platform::Twitch,
                vec![BadgeKind::Subscriber],
                "danylo_ua",
                "that transition was insane",
            ),
            ChatMessage::event(
                "14:02:41",
                Platform::Twitch,
                vec![BadgeKind::Subscriber],
                "maksym_dn",
                ChatBody::Subscription {
                    descriptor: " subscribed at tier 1".into(),
                    months: Some(5),
                    message: Some("keep it up, best stream on Twitch!".into()),
                    triggered: None,
                },
            ),
            ChatMessage::message(
                "14:02:52",
                Platform::Kick,
                vec![],
                "stream_fan_kyiv",
                "Kick chat checking in",
            ),
            ChatMessage::message(
                "14:03:04",
                Platform::YouTube,
                vec![BadgeKind::Bot],
                "nightbot42",
                "!discord — join the community server",
            ),
            ChatMessage::event(
                "14:03:18",
                Platform::Twitch,
                vec![],
                "lviv_gamer",
                ChatBody::Raid {
                    descriptor: " is raiding with".into(),
                    viewers: "512 viewers".into(),
                    triggered: Some("raid-welcome".into()),
                },
            ),
            ChatMessage::event(
                "14:03:29",
                Platform::Twitch,
                vec![BadgeKind::Vip],
                "haash_",
                ChatBody::Cheer {
                    descriptor: " cheered".into(),
                    bits: 500,
                    text: "take my bits, incredible play!".into(),
                },
            ),
        ];
        Self { messages }
    }

    /// The rows in arrival order (oldest first); the screen renders newest at the
    /// bottom and auto-scrolls there.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Appends one line. The bridge calls this inside `feed.update(cx, …)` and
    /// pairs it with `cx.notify()`; keeping the mutation free of `cx` leaves it
    /// directly exercisable.
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Decodes a bus event into a chat line, or `None` when the event is not a
    /// chat message. Provisional slice envelope: a `chat.message` event carrying
    /// `{ platform, author, text }`. No platform publishes this yet, so at runtime
    /// this returns `None` for every (timer) event — the seed is what shows. When
    /// real chat lands, this decode is replaced by the unified-chat-row mapping.
    pub fn message_from_event(event: &Event) -> Option<ChatMessage> {
        if event.kind != "chat.message" {
            return None;
        }
        let author = event.payload.get("author").and_then(|v| v.as_str())?;
        let text = event.payload.get("text").and_then(|v| v.as_str())?;
        let platform = match event.payload.get("platform").and_then(|v| v.as_str())? {
            "twitch" => Platform::Twitch,
            "youtube" => Platform::YouTube,
            "kick" => Platform::Kick,
            _ => return None,
        };
        Some(ChatMessage {
            timestamp: format_clock(event.timestamp.unix_timestamp()).into(),
            platform,
            badges: Vec::new(),
            username: author.to_owned().into(),
            body: ChatBody::Message(text.to_owned().into()),
            is_event: false,
            is_bot: false,
        })
    }
}

/// Formats a Unix timestamp as a wall-clock `HH:MM:SS` (UTC). Best-effort for the
/// slice — a locale/timezone-aware formatter lands with the real chat pipeline.
fn format_clock(unix_secs: i64) -> String {
    let secs = unix_secs.rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
