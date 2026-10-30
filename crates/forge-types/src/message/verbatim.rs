use std::fmt;

use serde::{Deserialize, Serialize};

use crate::redaction::RedactedText;

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VerbatimText(String);

impl VerbatimText {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for VerbatimText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("VerbatimText")
            .field(&RedactedText::new(&self.0))
            .finish()
    }
}
