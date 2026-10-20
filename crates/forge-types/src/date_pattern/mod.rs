mod cache;
mod current_language;
mod inline;
mod legacy;
mod names;
mod read;
mod render;
mod syntax;

use std::str::FromStr;

use time::OffsetDateTime;

pub use cache::{ParsedPattern, shared_date_pattern};
pub use current_language::{current_names_language, set_current_names_language};
pub(crate) use inline::render_inline_pattern;
pub use legacy::{LegacyTranslation, translate_legacy_pattern};
pub use names::NamesLanguage;
pub use read::DateTextError;
pub use render::PatternZone;
pub use syntax::DatePatternError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatePattern {
    pieces: Vec<syntax::PatternPiece>,
}

impl DatePattern {
    pub fn parse(pattern: &str) -> Result<Self, DatePatternError> {
        syntax::parse_pieces(pattern).map(|pieces| Self { pieces })
    }

    pub fn format(
        &self,
        value: OffsetDateTime,
        zone: PatternZone,
        language: NamesLanguage,
    ) -> String {
        render::render(&self.pieces, value, zone, language)
    }

    pub fn parse_text(
        &self,
        text: &str,
        zone: PatternZone,
        language: NamesLanguage,
    ) -> Result<OffsetDateTime, DateTextError> {
        read::read(&self.pieces, text, zone, language)
    }
}

impl FromStr for DatePattern {
    type Err = DatePatternError;

    fn from_str(pattern: &str) -> Result<Self, Self::Err> {
        Self::parse(pattern)
    }
}
