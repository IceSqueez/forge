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

    fn any_whisper_platform_enabled(&self) -> bool {
        PlatformId::ALL
            .iter()
            .filter(|platform| platform.supports_whispers())
            .any(|platform| !self.is_disabled(&IntegrationId::from_static(platform.as_str())))
    }
}

impl fmt::Display for IntegrationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    struct Disabled(HashSet<&'static str>);

    impl IntegrationAvailability for Disabled {
        fn is_disabled(&self, integration: &IntegrationId) -> bool {
            self.0.contains(integration.as_str())
        }
    }

    #[test]
    fn a_chat_platform_counts_as_enabled_until_twitch_youtube_and_kick_are_all_disabled() {
        for (disabled, expected) in [
            (&[][..], true),
            (&["obs"][..], true),
            (&["youtube", "kick"][..], true),
            (&["twitch", "kick"][..], true),
            (&["twitch", "youtube"][..], true),
            (&["twitch", "youtube", "obs", "vtube"][..], true),
            (&["twitch", "youtube", "kick"][..], false),
            (&["twitch", "youtube", "kick", "obs"][..], false),
        ] {
            let availability = Disabled(disabled.iter().copied().collect());
            assert_eq!(
                availability.any_chat_platform_enabled(),
                expected,
                "disabled {disabled:?}"
            );
        }
    }
}
