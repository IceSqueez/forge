use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::DiscordError;

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
    /// Color in `0x00RRGGBB` form. Values above `0x00FFFFFF` are rejected by `validate`.
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
            return Err(DiscordError::Validation(format!(
                "color 0x{c:06X} exceeds 0xFFFFFF"
            )));
        }
        check_len("title", self.title.as_deref(), 256)?;
        check_len("description", self.description.as_deref(), 4096)?;
        check_len("footer_text", self.footer_text.as_deref(), 2048)?;
        check_len("author_name", self.author_name.as_deref(), 256)?;
        if self.fields.len() > 25 {
            return Err(DiscordError::Validation(format!(
                "fields length {} exceeds 25",
                self.fields.len()
            )));
        }
        for field in &self.fields {
            check_no_nul("field.name", &field.name)?;
            check_no_nul("field.value", &field.value)?;
            check_len("field.name", Some(&field.name), 256)?;
            check_len("field.value", Some(&field.value), 1024)?;
        }
        let total = self.total_text_len();
        if total > 6000 {
            return Err(DiscordError::Validation(format!(
                "total embed text {total} chars exceeds 6000"
            )));
        }
        Ok(())
    }

    fn total_text_len(&self) -> usize {
        let mut n = 0usize;
        n += self.title.as_deref().map(str::len).unwrap_or(0);
        n += self.description.as_deref().map(str::len).unwrap_or(0);
        n += self.footer_text.as_deref().map(str::len).unwrap_or(0);
        n += self.author_name.as_deref().map(str::len).unwrap_or(0);
        for f in &self.fields {
            n += f.name.len();
            n += f.value.len();
        }
        n
    }
}

fn check_len(field: &str, value: Option<&str>, max: usize) -> Result<(), DiscordError> {
    let Some(s) = value else {
        return Ok(());
    };
    check_no_nul(field, s)?;
    if s.len() > max {
        return Err(DiscordError::Validation(format!(
            "{field} length {} exceeds {max}",
            s.len()
        )));
    }
    Ok(())
}

fn check_no_nul(field: &str, s: &str) -> Result<(), DiscordError> {
    if s.contains('\0') {
        return Err(DiscordError::Validation(format!(
            "{field} contains null byte"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    fn total_text_over_6000_rejected() {
        let e = DiscordEmbed {
            title: Some("a".repeat(256)),
            description: Some("b".repeat(4096)),
            footer_text: Some("c".repeat(2048)),
            ..Default::default()
        };
        assert!(matches!(e.validate(), Err(DiscordError::Validation(_))));
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
