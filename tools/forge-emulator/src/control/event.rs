use forge_events::{Event, EventSource};
use forge_types::EventId;
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;

#[derive(Deserialize)]
struct PushFrame {
    #[serde(rename = "timeStamp", with = "time::serde::rfc3339")]
    time_stamp: OffsetDateTime,
    event: PushEnvelope,
    data: Value,
}

#[derive(Deserialize)]
struct PushEnvelope {
    source: EventSource,
    #[serde(rename = "type")]
    kind: String,
    id: EventId,
    #[serde(rename = "causedBy", default)]
    caused_by: Option<EventId>,
    replay: bool,
}

pub(crate) fn decode_push(frame: Value) -> Result<Event, serde_json::Error> {
    let PushFrame {
        time_stamp,
        event,
        data,
    } = serde_json::from_value(frame)?;
    Ok(Event {
        id: event.id,
        source: event.source,
        kind: event.kind,
        timestamp: time_stamp,
        payload: data,
        caused_by: event.caused_by,
        replay: event.replay,
    })
}

pub(crate) fn decode_history_entry(entry: Value) -> Result<Event, serde_json::Error> {
    serde_json::from_value(entry)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;
    use time::format_description::well_known::Rfc3339;
    use time::macros::datetime;

    use super::*;

    type Mutation = fn(&mut Value);

    fn push_frame(event: &Event) -> Value {
        let mut envelope = json!({
            "source": event.source,
            "type": event.kind,
            "id": event.id,
            "replay": event.replay,
        });
        if let Some(cause) = event.caused_by {
            envelope["causedBy"] = json!(cause);
        }
        json!({
            "timeStamp": event.timestamp.format(&Rfc3339).unwrap(),
            "event": envelope,
            "data": event.payload,
        })
    }

    fn sample_events() -> Vec<Event> {
        let root = Event {
            id: EventId::new(),
            source: EventSource::Twitch,
            kind: "twitch.channel.chat.message".to_owned(),
            timestamp: datetime!(2026-09-13 12:00:00 UTC),
            payload: json!({ "user": { "login": "лісоруб" }, "text": "привіт 👋" }),
            caused_by: None,
            replay: false,
        };
        let replayed_effect = Event {
            id: EventId::new(),
            source: EventSource::YouTube,
            kind: "action.done".to_owned(),
            timestamp: datetime!(2026-09-13 12:00:00.123456789 UTC),
            payload: Value::Null,
            caused_by: Some(root.id),
            replay: true,
        };
        vec![root, replayed_effect]
    }

    #[test]
    fn push_and_history_shapes_decode_to_the_same_event() {
        for event in sample_events() {
            let expected = serde_json::to_value(&event).unwrap();

            let from_push = decode_push(push_frame(&event)).expect("push frame decodes");
            let from_history =
                decode_history_entry(expected.clone()).expect("history entry decodes");

            assert_eq!(serde_json::to_value(&from_push).unwrap(), expected, "push");
            assert_eq!(
                serde_json::to_value(&from_history).unwrap(),
                expected,
                "history"
            );
        }
    }

    #[test]
    fn push_frames_missing_or_malformed_envelope_fields_are_rejected() {
        let valid = push_frame(&sample_events()[0]);
        let mutations: [(&str, Mutation); 6] = [
            ("empty timestamp", |f| f["timeStamp"] = json!("")),
            ("unknown source", |f| f["event"]["source"] = json!("trovo")),
            ("snake_case kind key", |f| {
                let kind = f["event"]["type"].take();
                f["event"]["kind"] = kind;
                f["event"].as_object_mut().unwrap().remove("type");
            }),
            ("missing id", |f| {
                f["event"].as_object_mut().unwrap().remove("id");
            }),
            ("non-ulid cause", |f| {
                f["event"]["causedBy"] = json!("not-an-id")
            }),
            ("missing data", |f| {
                f.as_object_mut().unwrap().remove("data");
            }),
        ];
        for (label, mutate) in mutations {
            let mut frame = valid.clone();
            mutate(&mut frame);
            assert!(decode_push(frame).is_err(), "{label} decoded");
        }
    }
}
