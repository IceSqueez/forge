use forge_events::{Event, EventSource};
use gpui::SharedString;
use serde::Serialize;

const RING_CAP: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum EventFilter {
    #[default]
    All,
    Chat,
    Subs,
    Bits,
    Timers,
    Obs,
    Errors,
}

fn is_chat_message_kind(kind: &str) -> bool {
    matches!(
        kind,
        "chat.message"
            | "youtube.chat.message"
            | "kick.chat.message"
            | "youtube.chat.command"
            | "kick.chat.command"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct EventItem {
    pub id: SharedString,
    pub timestamp: SharedString,
    pub source: EventSource,
    pub kind: SharedString,
    pub summary: SharedString,
    pub result_tag: Option<SharedString>,
    pub is_error: bool,
    pub user_login: SharedString,
    pub user_platform: SharedString,
}

impl EventItem {
    pub fn matches(&self, filter: EventFilter) -> bool {
        let k = self.kind.as_ref();
        match filter {
            EventFilter::All => true,
            EventFilter::Chat => k.contains("chat") || k.contains("command"),
            EventFilter::Subs => {
                k.contains("sub") || k.contains("subscription") || k.contains("follow")
            }
            EventFilter::Bits => k.contains("cheer") || k.contains("bits") || k.contains("raid"),
            EventFilter::Timers => self.source == EventSource::Timer || k.contains("timer"),
            EventFilter::Obs => {
                self.source == EventSource::Obs || k.contains("scene") || k.contains("obs")
            }
            EventFilter::Errors => k.contains("error") || k.contains("fail"),
        }
    }
}

pub struct EventLog {
    items: Vec<EventItem>,
    paused: bool,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            paused: false,
        }
    }

    pub fn items(&self) -> &[EventItem] {
        &self.items
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn toggle_paused(&mut self) -> bool {
        self.paused = !self.paused;
        self.paused
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push(&mut self, item: EventItem) {
        if self.paused {
            return;
        }
        if self.items.len() >= RING_CAP {
            self.items.remove(0);
        }
        self.items.push(item);
    }

    pub fn item_from_event(event: &Event) -> Option<EventItem> {
        let ts = event.timestamp;
        let timestamp = format!(
            "{:02}:{:02}:{:02}.{:03}",
            ts.hour(),
            ts.minute(),
            ts.second(),
            ts.millisecond()
        );
        let kind = event.kind.clone();
        let is_error = kind.contains("error") || kind.contains("fail");
        let summary = Self::summarize(event);
        let (user_login, user_platform) = Self::acting_user(event);
        Some(EventItem {
            id: event.id.to_string().into(),
            timestamp: timestamp.into(),
            source: event.source,
            kind: kind.into(),
            summary: summary.into(),
            result_tag: None,
            is_error,
            user_login: user_login.into(),
            user_platform: user_platform.into(),
        })
    }

    pub(crate) fn summarize(event: &Event) -> String {
        let p = &event.payload;
        match event.kind.as_str() {
            k if is_chat_message_kind(k) => {
                let author = p.get("author").and_then(|v| v.as_str()).unwrap_or("?");
                let text = p.get("text").and_then(|v| v.as_str()).unwrap_or("");
                format!("{author}: {text}")
            }
            "timer.tick" => p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tick")
                .to_owned(),
            "action.start" => p
                .get("action_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_owned(),
            "action.done" => p
                .get("outcome")
                .and_then(|v| v.as_str())
                .unwrap_or("done")
                .to_owned(),
            _ => event.kind.clone(),
        }
    }

    fn acting_user(event: &Event) -> (String, String) {
        if !is_chat_message_kind(&event.kind) {
            return (String::new(), String::new());
        }
        let login = event
            .payload
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let platform = event
            .payload
            .get("platform")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        (login, platform)
    }
}
