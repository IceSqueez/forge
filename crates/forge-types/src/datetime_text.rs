use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn datetime_to_text(value: &OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_else(|_| value.to_string())
}

pub fn datetime_from_text(text: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(text.trim(), &Rfc3339).ok()
}
