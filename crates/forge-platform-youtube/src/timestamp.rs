use forge_types::{Variant, datetime_from_text};
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

pub(crate) fn parse_logged(text: &str) -> Option<OffsetDateTime> {
    let parsed = parse(text);
    if parsed.is_none() {
        tracing::debug!(
            len = text.len(),
            "youtube timestamp not parseable, variable omitted"
        );
    }
    parsed
}

pub(crate) fn variant(value: Option<OffsetDateTime>) -> Option<Variant> {
    value.map(Variant::Datetime)
}
