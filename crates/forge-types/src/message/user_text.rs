use serde::{Deserialize, Serialize};

use crate::message::code::MessageCode;
use crate::message::reference::MessageRef;
use crate::message::verbatim::VerbatimText;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserText {
    Message(MessageRef),
    Verbatim(VerbatimText),
}

impl UserText {
    pub fn unlocalized(text: impl Into<String>) -> Self {
        Self::Verbatim(VerbatimText::new(text))
    }

    pub fn code_or_text(&self) -> String {
        match self {
            Self::Message(message) => message.code().to_string(),
            Self::Verbatim(text) => text.as_str().to_owned(),
        }
    }
}

impl From<MessageCode> for UserText {
    fn from(code: MessageCode) -> Self {
        Self::Message(MessageRef::new(code))
    }
}

impl From<MessageRef> for UserText {
    fn from(message: MessageRef) -> Self {
        Self::Message(message)
    }
}

impl From<VerbatimText> for UserText {
    fn from(text: VerbatimText) -> Self {
        Self::Verbatim(text)
    }
}
