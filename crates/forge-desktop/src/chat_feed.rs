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
/// Starts empty and live: the boot-global bridge drains `chat.message` events off
/// the real bus and appends each decoded line through [`ChatFeed::push`]. Renders
/// empty-but-live until a platform connection publishes chat.
pub struct ChatFeed {
    messages: Vec<ChatMessage>,
}

impl ChatFeed {
    /// An empty feed. Lines arrive live over the bridge via [`ChatFeed::push`].
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
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
