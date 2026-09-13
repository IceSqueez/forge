use forge_events::Event;
use serde_json::{Map, Value};

use super::event::decode_push;

#[derive(Debug)]
pub enum Observation {
    Event(Event),
    /// The server's per-client buffer overflowed and this many events were never delivered.
    Dropped(u64),
    Undecodable {
        frame: String,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) code: Option<String>,
    pub(crate) message: String,
}

pub(crate) type ResponseOutcome = Result<Map<String, Value>, Refusal>;

#[derive(Debug)]
pub(crate) enum InboundFrame {
    Response {
        id: Option<String>,
        outcome: ResponseOutcome,
    },
    Observation(Observation),
}

pub(crate) fn classify(text: &str) -> InboundFrame {
    let undecodable = |reason: String| {
        InboundFrame::Observation(Observation::Undecodable {
            frame: text.to_owned(),
            reason,
        })
    };
    let mut fields = match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(fields)) => fields,
        Ok(_) => return undecodable("frame is not a JSON object".to_owned()),
        Err(e) => return undecodable(e.to_string()),
    };
    if let Some(status) = fields.remove("status") {
        return decode_response(fields, &status).unwrap_or_else(undecodable);
    }
    if let Some(dropped) = fields.get("dropped") {
        return match dropped.as_u64() {
            Some(count) => InboundFrame::Observation(Observation::Dropped(count)),
            None => undecodable(format!("dropped count is not a count: {dropped}")),
        };
    }
    if fields.contains_key("event") {
        return match decode_push(Value::Object(fields)) {
            Ok(event) => InboundFrame::Observation(Observation::Event(event)),
            Err(e) => undecodable(e.to_string()),
        };
    }
    undecodable("frame matches no known shape".to_owned())
}

fn decode_response(mut fields: Map<String, Value>, status: &Value) -> Result<InboundFrame, String> {
    let id = match fields.remove("id") {
        Some(Value::String(id)) => Some(id),
        Some(Value::Null) | None => None,
        Some(other) => return Err(format!("response id is not a string: {other}")),
    };
    let outcome = match status.as_str() {
        Some("ok") => Ok(fields),
        Some("error") => Err(refusal(fields.remove("error"))),
        _ => return Err(format!("unknown response status: {status}")),
    };
    Ok(InboundFrame::Response { id, outcome })
}

fn refusal(error: Option<Value>) -> Refusal {
    let error = error.unwrap_or(Value::Null);
    let code = error.get("code").and_then(Value::as_str).map(str::to_owned);
    let message = match error.get("message").and_then(Value::as_str) {
        Some(message) => message.to_owned(),
        None => error.to_string(),
    };
    Refusal { code, message }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::json;

    use super::*;

    fn response(text: &str) -> (Option<String>, ResponseOutcome) {
        match classify(text) {
            InboundFrame::Response { id, outcome } => (id, outcome),
            InboundFrame::Observation(observation) => {
                panic!("{text} classified as observation {observation:?}")
            }
        }
    }

    fn fields(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    #[test]
    fn responses_split_into_correlation_id_and_outcome() {
        let cases: [(&str, Option<&str>, ResponseOutcome); 5] = [
            (
                r#"{"id":"emu-1","status":"ok","authenticated":true}"#,
                Some("emu-1"),
                Ok(fields(json!({ "authenticated": true }))),
            ),
            (
                r#"{"id":"emu-2","status":"ok"}"#,
                Some("emu-2"),
                Ok(Map::new()),
            ),
            (
                r#"{"id":null,"status":"error","error":{"code":"INVALID_PAYLOAD","message":"unknown variant"}}"#,
                None,
                Err(Refusal {
                    code: Some("INVALID_PAYLOAD".to_owned()),
                    message: "unknown variant".to_owned(),
                }),
            ),
            (
                r#"{"status":"error","error":{"message":"no code"}}"#,
                None,
                Err(Refusal {
                    code: None,
                    message: "no code".to_owned(),
                }),
            ),
            (
                r#"{"id":"emu-3","status":"error","error":{"code":"X"}}"#,
                Some("emu-3"),
                Err(Refusal {
                    code: Some("X".to_owned()),
                    message: r#"{"code":"X"}"#.to_owned(),
                }),
            ),
        ];
        for (text, expected_id, expected_outcome) in cases {
            let (id, outcome) = response(text);
            assert_eq!(id.as_deref(), expected_id, "{text}");
            assert_eq!(outcome, expected_outcome, "{text}");
        }
    }

    #[test]
    fn a_drop_notice_carries_the_lost_event_count() {
        for count in [0_u64, 1, u64::MAX] {
            let text = json!({ "dropped": count }).to_string();
            assert!(
                matches!(classify(&text), InboundFrame::Observation(Observation::Dropped(n)) if n == count),
                "{text}"
            );
        }
    }

    #[test]
    fn malformed_frames_surface_as_undecodable_carrying_the_raw_frame() {
        for text in [
            "not json",
            "[1,2]",
            r#""ok""#,
            r#"{"id":"emu-1","status":"maybe"}"#,
            r#"{"id":5,"status":"ok"}"#,
            r#"{"dropped":-1}"#,
            r#"{"dropped":"3"}"#,
            r#"{"frame":"reload"}"#,
            r#"{"timeStamp":"2026-09-13T12:00:00Z","event":{"source":"twitch"},"data":{}}"#,
            "{}",
        ] {
            assert!(
                matches!(
                    classify(text),
                    InboundFrame::Observation(Observation::Undecodable { ref frame, .. }) if frame == text
                ),
                "{text}"
            );
        }
    }
}
