use std::collections::{HashMap, VecDeque};

use forge_events::{Event, EventSource};
use forge_types::{
    ChatModerationAction, ChatModerationPayload, ChatPayload, ChatSource, UnifiedChatRow,
};
use time::OffsetDateTime;

const DEDUP_WINDOW: usize = 500;

pub(crate) enum ChatRecord {
    Row(UnifiedChatRow),
    Moderation {
        source: ChatSource,
        action: ChatModerationAction,
        at: OffsetDateTime,
    },
}

/// Dedups rows per source on a sliding window of 500 `platform_msg_id`s.
#[derive(Default)]
pub(crate) struct ChatRecordMapper {
    dedup: HashMap<ChatSource, VecDeque<String>>,
}

impl ChatRecordMapper {
    pub(crate) fn map(&mut self, event: &Event) -> Option<ChatRecord> {
        if let Some(row) = try_map_chat_event(event, &mut self.dedup) {
            return Some(ChatRecord::Row(row));
        }
        try_map_moderation_event(event).map(|(source, action)| ChatRecord::Moderation {
            source,
            action,
            at: event.timestamp,
        })
    }
}

fn event_source_to_chat_source(src: EventSource) -> Option<ChatSource> {
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

fn try_map_moderation_event(ev: &Event) -> Option<(ChatSource, ChatModerationAction)> {
    let source = event_source_to_chat_source(ev.source)?;
    let value = ev.payload.get(ChatModerationPayload::KEY)?;
    let payload: ChatModerationPayload = serde_json::from_value(value.clone()).ok()?;
    Some((source, payload.action))
}

fn try_map_chat_event(
    ev: &Event,
    dedup: &mut HashMap<ChatSource, VecDeque<String>>,
) -> Option<UnifiedChatRow> {
    let source = event_source_to_chat_source(ev.source)?;
    let chat_value = ev.payload.get(ChatPayload::KEY)?;
    let payload: ChatPayload = serde_json::from_value(chat_value.clone()).ok()?;

    let window = dedup.entry(source).or_default();
    if window.contains(&payload.platform_msg_id) {
        return None;
    }
    if window.len() >= DEDUP_WINDOW {
        window.pop_front();
    }
    window.push_back(payload.platform_msg_id.clone());

    let author_color = payload
        .author_color
        .as_deref()
        .and_then(ChatPayload::parse_color);

    Some(UnifiedChatRow {
        id: payload.platform_msg_id,
        event_id: ev.id,
        source,
        received_at: ev.timestamp,
        author: payload.author,
        author_color,
        body_segments: payload.segments,
        badges: payload.badges,
        is_event: payload.is_event,
        event_detail: payload.event_detail,
        moderation: payload.moderation,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_events::{Event, EventSource};
    use forge_types::{ChatModerationAction, ChatModerationPayload, ChatSegment, ModerationMarks};
    use tokio::time::timeout;
    use tokio_stream::StreamExt as _;

    use super::*;
    use crate::{Config, EventBus, NullEventLogRepo};

    fn null_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    fn minimal_payload(msg_id: &str) -> ChatPayload {
        ChatPayload {
            platform_msg_id: msg_id.to_string(),
            author: "user".to_string(),
            author_color: None,
            segments: vec![ChatSegment::Text {
                text: "hi".to_string(),
            }],
            badges: vec![],
            is_event: false,
            event_detail: None,
            moderation: ModerationMarks::default(),
        }
    }

    fn chat_event(source: EventSource, msg_id: &str) -> Event {
        let payload = minimal_payload(msg_id);
        Event::new(
            source,
            "chat.message",
            serde_json::json!({ "_chat": serde_json::to_value(&payload).unwrap() }),
        )
    }

    #[tokio::test]
    async fn chat_stream_delivers_matching_event() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);
        bus.publish(chat_event(EventSource::Twitch, "msg-1"));
        let row = timeout(Duration::from_millis(200), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.id, "msg-1");
        assert_eq!(row.source, ChatSource::Twitch);
    }

    #[tokio::test]
    async fn chat_stream_filters_out_events_without_chat_key() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);
        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ "user": "foo", "text": "no chat key here" }),
        ));
        let result = timeout(Duration::from_millis(50), stream.next()).await;
        assert!(result.is_err(), "no row should arrive without _chat key");
    }

    #[tokio::test]
    async fn chat_stream_filters_out_non_chat_sources() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);
        let payload = minimal_payload("core-msg");
        bus.publish(Event::new(
            EventSource::Core,
            "action.start",
            serde_json::json!({ "_chat": serde_json::to_value(&payload).unwrap() }),
        ));
        let result = timeout(Duration::from_millis(50), stream.next()).await;
        assert!(result.is_err(), "Core source must be silently filtered");
    }

    #[tokio::test]
    async fn chat_stream_dedups_within_same_source() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);
        bus.publish(chat_event(EventSource::Twitch, "dup-id"));
        bus.publish(chat_event(EventSource::Twitch, "dup-id"));
        let first = timeout(Duration::from_millis(200), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.id, "dup-id");
        let second = timeout(Duration::from_millis(50), stream.next()).await;
        assert!(
            second.is_err(),
            "duplicate id in same source must be dropped"
        );
    }

    #[tokio::test]
    async fn chat_stream_allows_same_id_across_different_sources() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);
        bus.publish(chat_event(EventSource::Twitch, "shared-id"));
        bus.publish(chat_event(EventSource::YouTube, "shared-id"));
        let first = timeout(Duration::from_millis(200), stream.next())
            .await
            .unwrap()
            .unwrap();
        let second = timeout(Duration::from_millis(200), stream.next())
            .await
            .unwrap()
            .unwrap();
        let sources: std::collections::HashSet<ChatSource> =
            [first.source, second.source].into_iter().collect();
        assert!(sources.contains(&ChatSource::Twitch));
        assert!(sources.contains(&ChatSource::YouTube));
    }

    #[tokio::test]
    async fn chat_stream_readmits_an_id_evicted_from_the_dedup_window() {
        let bus = null_bus();
        let stream = chat_stream(Arc::clone(&bus));
        tokio::pin!(stream);

        let past_window = DEDUP_WINDOW + 1;
        for i in 0..past_window {
            bus.publish(chat_event(EventSource::Twitch, &format!("id-{i}")));
        }

        for _ in 0..past_window {
            timeout(Duration::from_millis(500), stream.next())
                .await
                .unwrap()
                .unwrap();
        }

        bus.publish(chat_event(EventSource::Twitch, "id-0"));
        let row = timeout(Duration::from_millis(200), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.id, "id-0");
    }

    const OBSERVER_CAPACITY: usize = 16;
    const BURST: usize = OBSERVER_CAPACITY * 4;

    type UnitStream = std::pin::Pin<Box<dyn Stream<Item = ()> + Send>>;
    type StreamBurstCase = (
        &'static str,
        fn(Arc<EventBus>) -> UnitStream,
        fn(usize) -> Event,
    );

    fn moderation_event(source: EventSource) -> Event {
        let payload = ChatModerationPayload {
            action: ChatModerationAction::ClearChat,
        };
        Event::new(
            source,
            "chat.moderation",
            serde_json::json!({
                (ChatModerationPayload::KEY): serde_json::to_value(&payload).unwrap(),
            }),
        )
    }

    #[tokio::test]
    async fn both_chat_streams_yield_every_event_of_a_burst_larger_than_the_observer_buffer() {
        let cases: [StreamBurstCase; 2] = [
            (
                "chat",
                |bus| Box::pin(chat_stream(bus).map(|_| ())),
                |i| chat_event(EventSource::Twitch, &format!("burst-{i}")),
            ),
            (
                "moderation",
                |bus| Box::pin(crate::chat_moderation_stream(bus).map(|_| ())),
                |_| moderation_event(EventSource::Twitch),
            ),
        ];

        for (label, build, event) in cases {
            let config = Config {
                bus_observer_capacity: OBSERVER_CAPACITY,
                ..Config::default()
            };
            let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
            let stream = build(Arc::clone(&bus));
            tokio::pin!(stream);

            for i in 0..BURST {
                bus.publish(event(i));
            }

            let yielded = timeout(
                Duration::from_millis(500),
                stream.as_mut().take(BURST).collect::<Vec<()>>(),
            )
            .await
            .map(|items| items.len());
            assert_eq!(
                yielded.ok(),
                Some(BURST),
                "{label} stream is a lossless consumer and must not lose a burst to lag"
            );
        }
    }
}
