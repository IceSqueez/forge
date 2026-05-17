use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    pub can_send_chat: bool,
    pub can_moderate: bool,
    pub can_subscribe_events: bool,
    pub can_polls: bool,
    pub can_predictions: bool,
    pub can_channel_points: bool,
    /// `true` if the platform connection uses unofficial / community-implementation endpoints
    /// (e.g. Kick before its public API). UI shows a prominent disclaimer in that case.
    pub limited: bool,
}

impl PlatformCapabilities {
    pub fn chat_only() -> Self {
        Self {
            can_send_chat: true,
            can_moderate: false,
            can_subscribe_events: false,
            can_polls: false,
            can_predictions: false,
            can_channel_points: false,
            limited: false,
        }
    }

    pub fn limited_chat_only() -> Self {
        Self {
            limited: true,
            ..Self::chat_only()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn chat_only_sets_only_send_chat() {
        let caps = PlatformCapabilities::chat_only();
        assert!(caps.can_send_chat);
        assert!(!caps.can_moderate);
        assert!(!caps.can_subscribe_events);
        assert!(!caps.can_polls);
        assert!(!caps.can_predictions);
        assert!(!caps.can_channel_points);
        assert!(!caps.limited);
    }

    #[test]
    fn limited_chat_only_sets_limited_flag() {
        let caps = PlatformCapabilities::limited_chat_only();
        assert!(caps.can_send_chat);
        assert!(caps.limited);
        assert!(!caps.can_moderate);
        assert!(!caps.can_subscribe_events);
        assert!(!caps.can_polls);
        assert!(!caps.can_predictions);
        assert!(!caps.can_channel_points);
    }

    #[test]
    fn serde_roundtrip_preserves_all_fields() {
        let original = PlatformCapabilities {
            can_send_chat: true,
            can_moderate: true,
            can_subscribe_events: true,
            can_polls: false,
            can_predictions: true,
            can_channel_points: false,
            limited: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: PlatformCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }
}
