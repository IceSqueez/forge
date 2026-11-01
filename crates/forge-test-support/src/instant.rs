use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

pub fn instant(rfc3339: &str) -> OffsetDateTime {
    OffsetDateTime::parse(rfc3339, &Rfc3339).expect("test instant is valid RFC 3339")
}
