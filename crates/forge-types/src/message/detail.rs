use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::redaction::RedactedText;

pub const MAX_UPSTREAM_DETAIL_CHARS: usize = 500;

const TRUNCATION_MARK: &str = "...";

#[derive(Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct UpstreamDetail(String);

impl UpstreamDetail {
    pub fn from_redacted(text: impl Into<String>) -> Self {
        let text = text.into();
        let trimmed = text.trim();
        match trimmed.char_indices().nth(MAX_UPSTREAM_DETAIL_CHARS) {
            Some((cut, _)) => Self(format!("{}{TRUNCATION_MARK}", &trimmed[..cut])),
            None if trimmed.len() == text.len() => Self(text),
            None => Self(trimmed.to_owned()),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for UpstreamDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("UpstreamDetail")
            .field(&RedactedText::new(&self.0))
            .finish()
    }
}

impl<'de> Deserialize<'de> for UpstreamDetail {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from_redacted)
    }
}
