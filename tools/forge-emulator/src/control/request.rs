use forge_events::EventSource;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// `None` on either axis subscribes to every value of that axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EventFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<EventSource>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

pub(crate) enum Request<'a> {
    Auth {
        token: &'a str,
    },
    Subscribe {
        events: &'a [EventFilter],
    },
    GetEvents {
        limit: u32,
    },
    GetInfo,
    DoAction {
        action_id: String,
        args: &'a Map<String, Value>,
    },
    SetGlobal {
        name: &'a str,
        value: &'a Value,
        persisted: bool,
    },
}

impl Request<'_> {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Self::Auth { .. } => "auth",
            Self::Subscribe { .. } => "subscribe",
            Self::GetEvents { .. } => "getEvents",
            Self::GetInfo => "getInfo",
            Self::DoAction { .. } => "doAction",
            Self::SetGlobal { .. } => "setGlobal",
        }
    }

    pub(crate) fn to_frame(&self, id: &str) -> String {
        let mut frame = match self {
            Self::Auth { token } => json!({ "token": token }),
            Self::Subscribe { events } => json!({ "events": events }),
            Self::GetEvents { limit } => json!({ "limit": limit }),
            Self::GetInfo => json!({}),
            Self::DoAction { action_id, args } => json!({ "actionId": action_id, "args": args }),
            Self::SetGlobal {
                name,
                value,
                persisted,
            } => json!({ "name": name, "value": value, "persisted": persisted }),
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
