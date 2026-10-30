use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MESSAGE_KEY_PREFIX: &str = "msg-";

const CODE_SEPARATOR: char = '.';
const KEY_SEPARATOR: char = '-';
const SEGMENT_JOINER: char = '_';

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MessageCodeError {
    #[error("message code must be `namespace.name` of lowercase snake_case segments: {0:?}")]
    Malformed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MessageCode {
    namespace: Cow<'static, str>,
    name: Cow<'static, str>,
}

impl MessageCode {
    pub const fn from_static(namespace: &'static str, name: &'static str) -> Self {
        Self {
            namespace: Cow::Borrowed(namespace),
            name: Cow::Borrowed(name),
        }
    }

    pub fn parse(raw: &str) -> Result<Self, MessageCodeError> {
        let malformed = || MessageCodeError::Malformed(raw.to_owned());
        let (namespace, name) = raw.split_once(CODE_SEPARATOR).ok_or_else(malformed)?;
        if !is_code_segment(namespace) || !is_code_segment(name) {
            return Err(malformed());
        }
        Ok(Self {
            namespace: Cow::Owned(namespace.to_owned()),
            name: Cow::Owned(name.to_owned()),
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_well_formed(&self) -> bool {
        is_code_segment(&self.namespace) && is_code_segment(&self.name)
    }

    pub fn ftl_key(&self) -> String {
        format!(
            "{MESSAGE_KEY_PREFIX}{}{KEY_SEPARATOR}{}",
            self.namespace, self.name
        )
    }
}

pub(crate) fn is_code_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == SEGMENT_JOINER)
}

impl fmt::Display for MessageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{CODE_SEPARATOR}{}", self.namespace, self.name)
    }
}

impl TryFrom<String> for MessageCode {
    type Error = MessageCodeError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

impl From<MessageCode> for String {
    fn from(code: MessageCode) -> Self {
        code.to_string()
    }
}
