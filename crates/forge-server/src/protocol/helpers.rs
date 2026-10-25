use forge_types::{ArgStack, Variant};

use crate::bus_adapter::EventFilter;

use super::envelope::WireEventFilter;

pub(crate) fn parse_wire_filter(wf: &WireEventFilter) -> EventFilter {
    let source = match wf.source.as_deref() {
        None | Some("*") => None,
        Some(s) => serde_json::from_value(serde_json::Value::String(s.to_owned())).ok(),
    };
    let kind = match wf.kind.as_deref() {
        None | Some("*") => None,
        Some(k) => Some(k.to_owned()),
    };
    EventFilter::new(source, kind)
}

pub(crate) fn variant_to_wire_value(v: &Variant) -> serde_json::Value {
    match v {
        Variant::Int(n) => serde_json::json!(n),
        Variant::Float(f) => serde_json::json!(f),
        Variant::Bool(b) => serde_json::json!(b),
        Variant::String(s) => serde_json::Value::String(s.clone()),
        Variant::Datetime(dt) => serde_json::Value::String(forge_types::datetime_to_text(dt)),
        Variant::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(variant_to_wire_value).collect())
        }
        Variant::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, val)| (k.clone(), variant_to_wire_value(val)))
                .collect(),
        ),
    }
}

pub(crate) fn valid_code_event_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn build_arg_stack(args: serde_json::Value) -> ArgStack {
    let obj = match args {
        serde_json::Value::Object(m) => m,
        _ => return ArgStack::new(),
    };
    obj.into_iter()
        .filter_map(|(k, v)| Variant::from_json(v).ok().map(|vv| (k, vv)))
        .fold(ArgStack::new(), |stack, (k, v)| stack.set(k, v))
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
                variant_to_wire_value(&nested),
                serde_json::json!([expected]),
                "value {value:?}"
            );
        }
    }
}
