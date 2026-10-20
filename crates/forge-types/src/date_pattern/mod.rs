mod legacy;
mod names;
mod render;
mod syntax;

use std::str::FromStr;

use time::OffsetDateTime;

pub use legacy::{LegacyTranslation, translate_legacy_pattern};
pub use names::NamesLanguage;
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
}

impl FromStr for DatePattern {
    type Err = DatePatternError;

    fn from_str(pattern: &str) -> Result<Self, Self::Err> {
        Self::parse(pattern)
    }
}
