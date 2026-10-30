use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{DiscordError, ValidationFault};

const MAX_FIELDS: usize = 25;
const MAX_TOTAL_TEXT: usize = 6000;
pub(crate) const MAX_CONTENT_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<u32>,
    pub fields: Vec<DiscordEmbedField>,
    pub thumbnail_url: Option<String>,
    pub image_url: Option<String>,
    pub footer_text: Option<String>,
    pub author_name: Option<String>,
    pub timestamp: Option<OffsetDateTime>,
}

impl DiscordEmbed {
    pub fn validate(&self) -> Result<(), DiscordError> {
        if let Some(c) = self.color.filter(|&c| c > 0x00FF_FFFF) {
            return Err(DiscordError::Validation(ValidationFault::ColorOutOfRange {
                color: c,
            }));
        }
        check_len("title", self.title.as_deref(), 256)?;
        check_len("description", self.description.as_deref(), 4096)?;
        check_len("footer_text", self.footer_text.as_deref(), 2048)?;
        check_len("author_name", self.author_name.as_deref(), 256)?;
        if self.fields.len() > MAX_FIELDS {
            return Err(DiscordError::Validation(ValidationFault::TooManyFields {
                count: self.fields.len(),
                max: MAX_FIELDS,
            }));
        }
        for field in &self.fields {
            check_no_nul("field.name", &field.name)?;
            check_no_nul("field.value", &field.value)?;
            check_len("field.name", Some(&field.name), 256)?;
            check_len("field.value", Some(&field.value), 1024)?;
        }
        let total = self.total_text_len();
        if total > MAX_TOTAL_TEXT {
            return Err(DiscordError::Validation(ValidationFault::TotalTooLong {
                total,
                max: MAX_TOTAL_TEXT,
            }));
        }
        Ok(())
    }

    fn total_text_len(&self) -> usize {
        let mut n = 0usize;
        n += self
            .title
            .as_deref()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        n += self
            .description
            .as_deref()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        n += self
            .footer_text
            .as_deref()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        n += self
            .author_name
            .as_deref()
            .map(|s| s.chars().count())
            .unwrap_or(0);
        for f in &self.fields {
            n += f.name.chars().count();
            n += f.value.chars().count();
        }
        n
    }
}

fn check_len(field: &'static str, value: Option<&str>, max: usize) -> Result<(), DiscordError> {
    let Some(s) = value else {
        return Ok(());
    };
    check_no_nul(field, s)?;
    let length = s.chars().count();
    if length > max {
        return Err(DiscordError::Validation(ValidationFault::PartTooLong {
            part: field,
            length,
            max,
        }));
    }
    Ok(())
}

fn check_no_nul(field: &'static str, s: &str) -> Result<(), DiscordError> {
    for c in s.chars() {
        if (c < '\x20' && c != '\n' && c != '\t') || c == '\x7f' {
            return Err(DiscordError::Validation(
                ValidationFault::ControlCharacter { part: field },
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_empty_embed_passes() {
        assert!(DiscordEmbed::default().validate().is_ok());
    }

    #[test]
    fn color_above_max_rejected() {
        let e = DiscordEmbed {
            color: Some(0x01_00_00_00),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn title_too_long_rejected() {
        let e = DiscordEmbed {
            title: Some("a".repeat(257)),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn description_too_long_rejected() {
        let e = DiscordEmbed {
            description: Some("b".repeat(4097)),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn too_many_fields_rejected() {
        let field = DiscordEmbedField {
            name: "n".to_owned(),
            value: "v".to_owned(),
            inline: false,
        };
        let e = DiscordEmbed {
            fields: vec![field; 26],
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn null_byte_in_title_rejected() {
        let e = DiscordEmbed {
            title: Some("bad\0char".to_owned()),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn other_control_char_in_description_rejected() {
        let e = DiscordEmbed {
            description: Some("bad\x01char".to_owned()),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    #[test]
    fn newline_and_tab_are_allowed() {
        let e = DiscordEmbed {
            description: Some("line1\nline2\ttabbed".to_owned()),
            ..Default::default()
        };
        assert!(e.validate().is_ok());
    }

    #[test]
    fn total_text_over_6000_rejected() {
        let e = DiscordEmbed {
            title: Some("a".repeat(256)),
            description: Some("b".repeat(4096)),
            footer_text: Some("c".repeat(2048)),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }

    fn cyrillic_fields(fields: usize, chars_per_field: usize) -> Vec<DiscordEmbedField> {
        vec![
            DiscordEmbedField {
                name: String::new(),
                value: "я".repeat(chars_per_field),
                inline: false,
            };
            fields
        ]
    }

    #[test]
    fn the_total_text_limit_counts_cyrillic_in_characters_not_bytes() {
        let at_limit = DiscordEmbed {
            fields: cyrillic_fields(12, 500),
            ..Default::default()
        };
        let over_limit = DiscordEmbed {
            title: Some("я".to_owned()),
            ..at_limit.clone()
        };

        assert!(at_limit.validate().is_ok(), "6000 characters must pass");
        assert!(matches!(
            over_limit.validate(),
            Err(DiscordError::Validation(ValidationFault::TotalTooLong {
                total: 6001,
                max: 6000,
            }))
        ));
    }

    #[test]
    fn a_title_is_measured_in_characters_not_bytes() {
        let title_of = |length: usize| DiscordEmbed {
            title: Some("я".repeat(length)),
            ..Default::default()
        };

        assert!(title_of(256).validate().is_ok());
        assert!(matches!(
            title_of(257).validate(),
            Err(DiscordError::Validation(ValidationFault::PartTooLong {
                part: "title",
                length: 257,
                max: 256,
            }))
        ));
    }

    #[test]
    fn embed_field_value_too_long_rejected() {
        let e = DiscordEmbed {
            fields: vec![DiscordEmbedField {
                name: "field".to_owned(),
                value: "x".repeat(1025),
                inline: false,
            }],
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
    }
}
