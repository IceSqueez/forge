use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::platform::PlatformId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IntegrationId(Cow<'static, str>);

impl IntegrationId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(Cow::Owned(id.into()))
    }

    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub trait IntegrationAvailability: Send + Sync {
    fn is_disabled(&self, integration: &IntegrationId) -> bool;

    fn any_chat_platform_enabled(&self) -> bool {
        PlatformId::ALL
            .iter()
            .any(|platform| !self.is_disabled(&IntegrationId::from_static(platform.as_str())))
    }
}

impl fmt::Display for IntegrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
