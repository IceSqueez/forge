use std::collections::HashSet;
use std::str::FromStr;

use forge_types::{ArgStack, Variant};

use crate::bus_adapter::EventFilter;

use super::envelope::{WireEventFilter, WsErrorCode, WsResponse};

const RUNTIME_FAILURE_MESSAGE: &str = "the request could not be completed";

#[derive(Debug, thiserror::Error)]
#[error("unknown event source: {0}")]
pub(crate) struct UnknownEventSource(String);

pub(crate) fn parse_wire_filters(
    filters: &[WireEventFilter],
) -> Result<HashSet<EventFilter>, UnknownEventSource> {
    filters.iter().map(parse_wire_filter).collect()
}

fn parse_wire_filter(wf: &WireEventFilter) -> Result<EventFilter, UnknownEventSource> {
    let source = match wf.source.as_deref() {
        None | Some("*") => None,
        Some(s) => Some(
            serde_json::from_value(serde_json::Value::String(s.to_owned()))
                .map_err(|_| UnknownEventSource(s.to_owned()))?,
        ),
    };
    let kind = match wf.kind.as_deref() {
        None | Some("*") => None,
        Some(k) => Some(k.to_owned()),
    };
    Ok(EventFilter::new(source, kind))
}

pub(super) const MAX_CODE_EVENT_NAME_LEN: usize = 64;

pub(crate) fn valid_code_event_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_CODE_EVENT_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

pub(crate) fn build_arg_stack(args: serde_json::Value) -> ArgStack {
    let obj = match args {
        serde_json::Value::Object(m) => m,
        serde_json::Value::Null => return ArgStack::new(),
        other => {
            tracing::warn!(
                shape = json_kind(&other),
                "action args ignored: not a JSON object"
            );
            return ArgStack::new();
        }
    };
    let received = obj.len();
    let accepted: Vec<(String, Variant)> = obj
        .into_iter()
        .filter_map(|(key, value)| {
            Variant::from_plain_json(value)
                .ok()
                .map(|variant| (key, variant))
        })
        .collect();
    let dropped = received - accepted.len();
    if dropped > 0 {
        tracing::warn!(dropped, "action args dropped: not representable");
    }
    accepted
        .into_iter()
        .fold(ArgStack::new(), |stack, (k, v)| stack.set(k, v))
}

pub(super) fn runtime_failure(error: &impl std::fmt::Display) -> WsResponse {
    tracing::warn!(%error, "websocket request failed");
    WsResponse::error(
        WsErrorCode::RuntimeError,
        RUNTIME_FAILURE_MESSAGE.to_owned(),
    )
}

pub(super) fn parse_id<T: FromStr>(raw: &str, refusal: &str) -> Result<T, WsResponse> {
    raw.parse()
        .map_err(|_| WsResponse::error(WsErrorCode::InvalidPayload, refusal.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

    fn at(year: i32, nanos: u32, offset: (i8, i8, i8)) -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(year, Month::March, 5).unwrap(),
            Time::from_hms_nano(14, 7, 9, nanos).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(offset.0, offset.1, offset.2).unwrap())
    }

    #[test]
    fn wire_value_writes_a_datetime_as_the_same_text_every_other_renderer_uses() {
        for (value, expected) in [
            (at(2026, 0, (0, 0, 0)), "2026-03-05T14:07:09Z"),
            (at(2026, 0, (-5, -30, 0)), "2026-03-05T14:07:09-05:30"),
            (at(2026, 500_000_000, (0, 0, 0)), "2026-03-05T14:07:09.5Z"),
            (at(-1, 0, (0, 0, 0)), "-0001-03-05 14:07:09.0 +00:00:00"),
        ] {
            let nested = Variant::Array(vec![Variant::Datetime(value)]);
            assert_eq!(
                nested.to_plain_json(),
                serde_json::json!([expected]),
                "value {value:?}"
            );
        }
    }

    #[test]
    fn a_runtime_failure_tells_the_client_nothing_about_its_cause() {
        let response = runtime_failure(&"/home/streamer/forge/forge.db is locked");

        let WsResponse::Error { code, message } = response else {
            panic!("a runtime failure must answer with an error");
        };
        assert_eq!(code, WsErrorCode::RuntimeError);
        assert_eq!(message, RUNTIME_FAILURE_MESSAGE);
    }
}
