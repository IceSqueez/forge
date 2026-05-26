use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerPlatform {
    Twitch,
    YouTube,
    Kick,
    Trovo,
}

impl ViewerPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::YouTube => "youtube",
            Self::Kick => "kick",
            Self::Trovo => "trovo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "twitch" => Some(Self::Twitch),
            "youtube" => Some(Self::YouTube),
            "kick" => Some(Self::Kick),
            "trovo" => Some(Self::Trovo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewer {
    pub viewer_id: String,
    pub platform: ViewerPlatform,
    pub username: String,
    pub first_seen_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub message_count: u64,
    pub custom_greeting: bool,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ViewerRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Viewer>, StorageError>;
    async fn get(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
    ) -> Result<Option<Viewer>, StorageError>;
    /// Upsert called per chat message: if row exists, bump `message_count` + update
    /// `last_seen_at` + refresh `username`; else create with `first_seen_at = now`.
    async fn record_message(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
        username: &str,
    ) -> Result<(), StorageError>;
    async fn set_custom_greeting(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
        enabled: bool,
    ) -> Result<bool, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ViewerRepo) {}

    #[test]
    fn platform_roundtrip() {
        for p in [
            ViewerPlatform::Twitch,
            ViewerPlatform::YouTube,
            ViewerPlatform::Kick,
            ViewerPlatform::Trovo,
        ] {
            assert_eq!(ViewerPlatform::parse(p.as_str()), Some(p));
        }
    }

    #[test]
    fn platform_parse_unknown_is_none() {
        assert_eq!(ViewerPlatform::parse("discord"), None);
    }
}
