use forge_components::{BadgeKind, ChatBody, Platform};
use forge_events::Event;
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

fn format_clock(unix_secs: i64) -> String {
    let secs = unix_secs.rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}
