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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_events::{Event, EventSource};
    use forge_types::{ChatModerationAction, ChatModerationPayload, ChatSegment, ModerationMarks};

    use super::*;

    fn chat_event(source: EventSource, msg_id: &str) -> Event {
        let payload = ChatPayload {
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
        };
        Event::new(
            source,
            "chat.message",
            serde_json::json!({ (ChatPayload::KEY): serde_json::to_value(&payload).unwrap() }),
        )
    }

    fn moderation_event(source: EventSource, action: ChatModerationAction) -> Event {
        let payload = ChatModerationPayload { action };
        Event::new(
            source,
            "chat.moderation",
            serde_json::json!({ (ChatModerationPayload::KEY): serde_json::to_value(&payload).unwrap() }),
        )
    }

    fn row_id(record: Option<ChatRecord>) -> Option<String> {
        match record {
            Some(ChatRecord::Row(row)) => Some(row.id),
            _ => None,
        }
    }

    #[test]
    fn a_chat_event_maps_to_a_row_carrying_its_event_identity() {
        let event = chat_event(EventSource::Twitch, "msg-1");

        let Some(ChatRecord::Row(row)) = ChatRecordMapper::default().map(&event) else {
            panic!("a chat event must map to a row");
        };

        assert_eq!(
            (row.id.as_str(), row.source, row.event_id, row.received_at),
            ("msg-1", ChatSource::Twitch, event.id, event.timestamp)
        );
    }

    #[test]
    fn events_that_are_not_chat_rows_or_marks_map_to_nothing() {
        let unrelated = [
            Event::new(
                EventSource::Twitch,
                "chat.message",
                serde_json::json!({ "user": "foo", "text": "no chat key here" }),
            ),
            Event::new(
                EventSource::Twitch,
                "chat.message",
                serde_json::json!({ (ChatPayload::KEY): { "platform_msg_id": 7 } }),
            ),
            chat_event(EventSource::Core, "core-msg"),
            moderation_event(EventSource::Core, ChatModerationAction::ClearChat),
        ];

        let mut mapper = ChatRecordMapper::default();
        for event in &unrelated {
            assert!(
                mapper.map(event).is_none(),
                "{:?} must be dropped",
                event.kind
            );
        }
    }

    #[test]
    fn a_repeated_message_id_from_the_same_source_is_dropped() {
        let mut mapper = ChatRecordMapper::default();
        assert!(row_id(mapper.map(&chat_event(EventSource::Twitch, "dup"))).is_some());

        assert!(
            mapper
                .map(&chat_event(EventSource::Twitch, "dup"))
                .is_none()
        );
    }

    #[test]
    fn the_same_message_id_from_another_source_is_kept() {
        let mut mapper = ChatRecordMapper::default();
        mapper.map(&chat_event(EventSource::Twitch, "shared"));

        assert_eq!(
            row_id(mapper.map(&chat_event(EventSource::YouTube, "shared"))).as_deref(),
            Some("shared")
        );
    }

    #[test]
    fn an_id_stays_deduplicated_until_the_window_overflows_by_one() {
        let mut mapper = ChatRecordMapper::default();
        for i in 0..DEDUP_WINDOW {
            mapper.map(&chat_event(EventSource::Twitch, &format!("id-{i}")));
        }
        assert!(
            mapper
                .map(&chat_event(EventSource::Twitch, "id-0"))
                .is_none(),
            "a full window still remembers its oldest id"
        );

        mapper.map(&chat_event(EventSource::Twitch, "one-past-the-window"));

        assert_eq!(
            row_id(mapper.map(&chat_event(EventSource::Twitch, "id-0"))).as_deref(),
            Some("id-0")
        );
    }

    #[test]
    fn a_moderation_event_maps_to_a_mark_stamped_with_the_event_time() {
        let action = ChatModerationAction::RemoveUser {
            user_name: "bob".to_string(),
            timeout: true,
        };
        let event = moderation_event(EventSource::Kick, action.clone());

        let Some(ChatRecord::Moderation {
            source,
            action: mapped,
            at,
        }) = ChatRecordMapper::default().map(&event)
        else {
            panic!("a moderation event must map to a mark");
        };

        assert_eq!(
            (source, mapped, at),
            (ChatSource::Kick, action, event.timestamp)
        );
    }
}
