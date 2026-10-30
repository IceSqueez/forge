use serde::{Deserialize, Serialize};

use crate::message::reference::MessageRef;
use crate::message::verbatim::VerbatimText;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserText {
    Message(MessageRef),
    Verbatim(VerbatimText),
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
