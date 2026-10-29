use std::collections::VecDeque;

use forge_events::{ActionDone, ActionStarted, Event, EventSource, TimerTick, TypedEvent};
use forge_types::{ChatPayload, ChatSegment};
use gpui::SharedString;
use serde::Serialize;
use time::OffsetDateTime;

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

pub(crate) fn is_chat_message_kind(kind: &str) -> bool {
    matches!(
        kind,
        "twitch.channel.chat.message"
            | "youtube.chat.message"
            | "kick.chat.message.sent"
            | "kick.chat.command"
    )
}

#[derive(Clone, Debug, Serialize)]
pub struct EventItem {
    pub id: SharedString,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub source: EventSource,
    pub kind: SharedString,
    pub summary: SharedString,
    pub caused_by: Option<SharedString>,
    pub sub_action_count: Option<u32>,
    pub total_ms: Option<f64>,
    pub is_error: bool,
    pub replay: bool,
    pub user_login: SharedString,
    pub user_platform: SharedString,
    pub payload: serde_json::Value,
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
    items: VecDeque<EventItem>,
    paused: bool,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            paused: false,
        }
    }

    pub fn items(&self) -> &VecDeque<EventItem> {
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
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    pub fn item_from_event(event: &Event) -> Option<EventItem> {
        let kind = event.kind.clone();
        let is_error = kind.contains("error") || kind.contains("fail");
        let summary = Self::summarize(event);
        let caused_by = event.caused_by.map(|id| SharedString::from(id.to_string()));
        let p = &event.payload;
        let sub_action_count = p
            .get("sub_action_count")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as u32);
        let total_ms = p.get("total_ms").and_then(serde_json::Value::as_f64);
        let (user_login, user_platform) = Self::acting_user(event);
        Some(EventItem {
            id: event.id.to_string().into(),
            timestamp: event.timestamp,
            source: event.source,
            kind: kind.into(),
            summary: summary.into(),
            caused_by,
            sub_action_count,
            total_ms,
            is_error,
            replay: event.replay,
            user_login: user_login.into(),
            user_platform: user_platform.into(),
            payload: event.payload.clone(),
        })
    }

    pub(crate) fn summarize(event: &Event) -> String {
        let p = &event.payload;
        match event.kind.as_str() {
            k if is_chat_message_kind(k) => match Self::chat_envelope(event) {
                Some(chat) => format!("{}: {}", chat.author, Self::chat_text(&chat.segments)),
                None => String::new(),
            },
            TimerTick::KIND => p
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("tick")
                .to_owned(),
            ActionStarted::KIND => p
                .get("action_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_owned(),
            ActionDone::KIND => p
                .get("outcome")
                .and_then(|v| v.as_str())
                .unwrap_or("done")
                .to_owned(),
            _ => {
                for key in [
                    "message", "name", "detail", "reason", "status", "state", "scene", "value",
                ] {
                    if let Some(s) = p.get(key).and_then(|v| v.as_str())
                        && !s.is_empty()
                    {
                        return s.to_owned();
                    }
                }
                String::new()
            }
        }
    }

    fn acting_user(event: &Event) -> (String, String) {
        if !is_chat_message_kind(&event.kind) {
            return (String::new(), String::new());
        }
        let login = Self::chat_envelope(event)
            .map(|chat| chat.author)
            .unwrap_or_default();
        let platform = event
            .source
            .to_platform_id()
            .map(|id| id.as_str().to_owned())
            .unwrap_or_default();
        (login, platform)
    }

    fn chat_envelope(event: &Event) -> Option<ChatPayload> {
        let value = event.payload.get(ChatPayload::KEY)?;
        serde_json::from_value(value.clone()).ok()
    }

    fn chat_text(segments: &[ChatSegment]) -> String {
        let mut out = String::new();
        for segment in segments {
            match segment {
                ChatSegment::Text { text } => out.push_str(text),
                ChatSegment::Mention { username } => {
                    out.push('@');
                    out.push_str(username);
                }
                ChatSegment::Emote { name, .. } => out.push_str(name),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWITCH_CHAT: &str =
        include_str!("../../forge-platform-twitch/tests/fixtures/published_chat_message.json");
    const YOUTUBE_CHAT: &str =
        include_str!("../../forge-platform-youtube/tests/fixtures/published_chat_message.json");
    const KICK_CHAT: &str =
        include_str!("../../forge-platform-kick/tests/fixtures/published_chat_message.json");

    fn published_event(fixture: &str) -> Event {
        let fixture: serde_json::Value = serde_json::from_str(fixture).unwrap();
        Event::new(
            serde_json::from_value(fixture["source"].clone()).unwrap(),
            fixture["kind"].as_str().unwrap(),
            fixture["payload"].clone(),
        )
    }

    #[test]
    fn chat_summary_shows_author_and_text_of_each_platforms_published_message() {
        for (fixture, expected) in [
            (TWITCH_CHAT, "Viewer32: Hi chat @streamer Kappa"),
            (YOUTUBE_CHAT, "Глядач Ютубу: привіт з ютубу"),
            (KICK_CHAT, "kick-viewer: gg kick chat"),
        ] {
            let event = published_event(fixture);
            assert_eq!(EventLog::summarize(&event), expected, "kind={}", event.kind);
        }
    }

    #[test]
    fn chat_summary_is_empty_when_the_chat_envelope_is_missing_or_unreadable() {
        for envelope in [None, Some(serde_json::json!("not an envelope"))] {
            let mut event = published_event(TWITCH_CHAT);
            let payload = event.payload.as_object_mut().unwrap();
            payload.remove(ChatPayload::KEY);
            if let Some(envelope) = &envelope {
                payload.insert(ChatPayload::KEY.to_owned(), envelope.clone());
            }
            assert_eq!(EventLog::summarize(&event), "", "envelope={envelope:?}");
        }
    }

    #[test]
    fn acting_user_is_the_chat_author_on_the_platform_that_published_the_message() {
        for (fixture, author, platform) in [
            (TWITCH_CHAT, "Viewer32", "twitch"),
            (YOUTUBE_CHAT, "Глядач Ютубу", "youtube"),
            (KICK_CHAT, "kick-viewer", "kick"),
        ] {
            let event = published_event(fixture);
            assert_eq!(
                EventLog::acting_user(&event),
                (author.to_owned(), platform.to_owned()),
                "kind={}",
                event.kind
            );
        }
    }

    #[test]
    fn an_item_is_marked_replay_exactly_when_its_event_is() {
        for replay in [false, true] {
            let mut event = published_event(TWITCH_CHAT);
            event.replay = replay;

            let item = EventLog::item_from_event(&event).unwrap();

            assert_eq!(item.replay, replay);
        }
    }

    #[test]
    fn exported_items_keep_the_event_instant_as_iso_utc() {
        let mut event = published_event(TWITCH_CHAT);
        event.timestamp = time::OffsetDateTime::from_unix_timestamp(1_773_617_400).unwrap()
            + time::Duration::milliseconds(90);
        let item = EventLog::item_from_event(&event).unwrap();

        let exported = serde_json::to_value(&item).unwrap();

        assert_eq!(exported["timestamp"], "2026-03-15T23:30:00.09Z");
    }
}
