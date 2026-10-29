use std::collections::VecDeque;

use forge_components::tr;
use forge_events::{Event, EventSource};
use forge_registry::TriggerCategory;
use forge_types::{ChatPayload, ChatSegment};
use gpui::SharedString;
use serde::Serialize;
use time::OffsetDateTime;

use crate::event_categories::EventCategories;
use crate::runtime_event::{self, RuntimeEvent};

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
    #[serde(skip)]
    pub typed: Option<RuntimeEvent>,
    #[serde(skip)]
    pub categories: Vec<TriggerCategory>,
}

impl EventItem {
    pub fn matches(&self, filter: EventFilter) -> bool {
        match filter {
            EventFilter::All => true,
            EventFilter::Chat => self.is_chat_activity(),
            EventFilter::Subs => self.has_category(TriggerCategory::Subscriptions),
            EventFilter::Bits => {
                self.has_category(TriggerCategory::Bits)
                    || self.has_category(TriggerCategory::Raids)
            }
            EventFilter::Timers => {
                self.source == EventSource::Timer
                    || matches!(self.typed, Some(RuntimeEvent::TimerTick(_)))
            }
            EventFilter::Obs => {
                self.source == EventSource::Obs || self.has_category(TriggerCategory::Obs)
            }
            EventFilter::Errors => self.is_error,
        }
    }

    pub fn has_category(&self, category: TriggerCategory) -> bool {
        self.categories.contains(&category)
    }

    pub fn is_warning(&self) -> bool {
        !self.is_error && self.typed.as_ref().is_some_and(RuntimeEvent::is_warning)
    }

    fn is_chat_activity(&self) -> bool {
        self.has_category(TriggerCategory::Chat)
            || matches!(
                self.typed,
                Some(
                    RuntimeEvent::ChatMessage
                        | RuntimeEvent::ChatSent(_)
                        | RuntimeEvent::ChatSendFailed(_)
                        | RuntimeEvent::WhisperSent { .. }
                        | RuntimeEvent::CommandMatched { .. }
                        | RuntimeEvent::SenderFallback(_)
                )
            )
    }
}

pub struct EventLog {
    items: VecDeque<EventItem>,
    paused: bool,
    categories: EventCategories,
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            paused: false,
            categories: EventCategories::default(),
        }
    }

    pub fn set_categories(&mut self, categories: EventCategories) {
        self.categories = categories;
    }

    pub fn record(&mut self, event: &Event, typed: Option<RuntimeEvent>) {
        if self.paused {
            return;
        }
        let categories = self.categories.of(event);
        self.push(Self::item_from_event(event, typed, categories));
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

    pub fn item_from_event(
        event: &Event,
        typed: Option<RuntimeEvent>,
        mut categories: Vec<TriggerCategory>,
    ) -> EventItem {
        if runtime_event::is_twitch_follow(event)
            && !categories.contains(&TriggerCategory::Subscriptions)
        {
            categories.push(TriggerCategory::Subscriptions);
        }
        let summary = Self::summarize(event, typed.as_ref());
        let (user_login, user_platform) = Self::acting_user(event, typed.as_ref());
        let (sub_action_count, total_ms) = match &typed {
            Some(RuntimeEvent::ActionStarted(started)) => {
                (u32::try_from(started.sub_action_count).ok(), None)
            }
            Some(RuntimeEvent::ActionDone(done)) => (None, Some(done.total_ms as f64)),
            _ => (None, None),
        };
        EventItem {
            id: event.id.to_string().into(),
            timestamp: event.timestamp,
            source: event.source,
            kind: event.kind.clone().into(),
            summary: summary.into(),
            caused_by: event.caused_by.map(|id| SharedString::from(id.to_string())),
            sub_action_count,
            total_ms,
            is_error: typed.as_ref().is_some_and(RuntimeEvent::is_failure),
            replay: event.replay,
            user_login: user_login.into(),
            user_platform: user_platform.into(),
            payload: event.payload.clone(),
            typed,
            categories,
        }
    }

    pub(crate) fn summarize(event: &Event, typed: Option<&RuntimeEvent>) -> String {
        match typed {
            Some(RuntimeEvent::ChatMessage) => match Self::chat_envelope(event) {
                Some(chat) => format!("{}: {}", chat.author, Self::chat_text(&chat.segments)),
                None => String::new(),
            },
            Some(RuntimeEvent::TimerTick(tick)) => tr!(
                "triggers_timer_every",
                minutes = i64::from(tick.interval_minutes)
            ),
            Some(RuntimeEvent::ActionStarted(started)) => started.action_name.clone(),
            Some(RuntimeEvent::ActionDone(done)) => serde_json::to_value(done.outcome)
                .ok()
                .and_then(|outcome| outcome.as_str().map(str::to_owned))
                .unwrap_or_default(),
            _ => Self::first_text_field(&event.payload),
        }
    }

    fn first_text_field(payload: &serde_json::Value) -> String {
        for key in [
            "message", "name", "detail", "reason", "status", "state", "scene", "value",
        ] {
            if let Some(s) = payload.get(key).and_then(|v| v.as_str())
                && !s.is_empty()
            {
                return s.to_owned();
            }
        }
        String::new()
    }

    fn acting_user(event: &Event, typed: Option<&RuntimeEvent>) -> (String, String) {
        if !matches!(typed, Some(RuntimeEvent::ChatMessage)) {
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
