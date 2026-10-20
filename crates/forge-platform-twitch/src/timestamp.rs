use forge_types::{ArgStack, Variant, datetime_from_text};
use time::OffsetDateTime;
use time::PrimitiveDateTime;
use time::format_description::well_known::Iso8601;

pub(crate) fn parse(text: &str) -> Option<OffsetDateTime> {
    datetime_from_text(text).or_else(|| {
        PrimitiveDateTime::parse(text.trim(), &Iso8601::DEFAULT)
            .ok()
            .map(PrimitiveDateTime::assume_utc)
    })
}

pub(crate) fn variant(text: &str) -> Option<Variant> {
    let parsed = parse(text);
    if parsed.is_none() {
        tracing::debug!(
            len = text.len(),
            "twitch timestamp not parseable, variable omitted"
        );
    }
    parsed.map(Variant::Datetime)
}

pub(crate) fn variant_of(value: Option<&serde_json::Value>) -> Option<Variant> {
    value.and_then(|raw| raw.as_str()).and_then(variant)
}

pub(crate) fn set(stack: ArgStack, name: &str, value: Option<Variant>) -> ArgStack {
    match value {
        Some(value) => stack.set(name.to_owned(), value),
        None => stack,
    }
}
