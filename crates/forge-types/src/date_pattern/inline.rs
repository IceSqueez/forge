use time::OffsetDateTime;

use super::PatternZone;
use super::cache::cached_pattern;
use super::current_language::current_names_language;
use crate::datetime_text::datetime_to_text;

pub(crate) fn render_inline_pattern(value: OffsetDateTime, pattern: &str) -> String {
    let cached = cached_pattern(pattern);
    match cached.parsed.as_ref() {
        Ok(parsed) => parsed.format(value, PatternZone::Local, current_names_language()),
        Err(error) => {
            if cached.newly_parsed {
                tracing::warn!(
                    pattern,
                    %error,
                    "template date format is invalid; rendering the value in canonical form"
                );
            }
            datetime_to_text(&value)
        }
    }
}
