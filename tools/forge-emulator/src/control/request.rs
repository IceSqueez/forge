use forge_events::EventSource;
use serde::Serialize;
use serde_json::{Value, json};

/// `None` on either axis subscribes to every value of that axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EventFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<EventSource>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

pub(crate) enum Request<'a> {
    Auth { token: &'a str },
    Subscribe { events: &'a [EventFilter] },
    GetEvents { limit: u32 },
}

impl Request<'_> {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Self::Auth { .. } => "auth",
            Self::Subscribe { .. } => "subscribe",
            Self::GetEvents { .. } => "getEvents",
        }
    }

    pub(crate) fn to_frame(&self, id: &str) -> String {
        let mut frame = match self {
            Self::Auth { token } => json!({ "token": token }),
            Self::Subscribe { events } => json!({ "events": events }),
            Self::GetEvents { limit } => json!({ "limit": limit }),
        };
        if let Value::Object(fields) = &mut frame {
            fields.insert("id".to_owned(), Value::String(id.to_owned()));
            fields.insert(
                "request".to_owned(),
                Value::String(self.method().to_owned()),
            );
        }
        frame.to_string()
    }
}
