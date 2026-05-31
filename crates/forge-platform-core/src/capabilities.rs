use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformCapabilities {
    pub can_send_chat: bool,
    pub can_moderate: bool,
    pub can_subscribe_events: bool,
    pub can_polls: bool,
    pub can_predictions: bool,
    pub can_channel_points: bool,
    /// `true` if the platform connection uses unofficial / community-implementation endpoints.
    /// UI shows a prominent disclaimer in that case.
    pub limited: bool,
    /// Non-empty only when `limited` is true. User-facing single sentence explaining why
    /// the platform uses unofficial or restricted endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limited_reason: Option<String>,
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
            limited_reason: None,
        }
    }

    /// Returns a read-only capability set for platforms using unofficial endpoints.
    ///
    /// Sets `limited = true` and all send/moderation flags to false. `reason` is
    /// the user-visible explanation surfaced in the UI disclaimer.
    pub fn limited_read_only(reason: impl Into<String>) -> Self {
        Self {
            can_send_chat: false,
            can_moderate: false,
            can_subscribe_events: false,
            can_polls: false,
            can_predictions: false,
            can_channel_points: false,
            limited: true,
            limited_reason: Some(reason.into()),
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
        assert!(caps.limited_reason.is_none());
    }

    #[test]
    fn limited_read_only_sets_limited_flag_and_reason() {
        let caps = PlatformCapabilities::limited_read_only("community WS only");
        assert!(caps.limited);
        assert_eq!(caps.limited_reason.as_deref(), Some("community WS only"));
        assert!(!caps.can_send_chat);
        assert!(!caps.can_moderate);
        assert!(!caps.can_subscribe_events);
        assert!(!caps.can_polls);
        assert!(!caps.can_predictions);
        assert!(!caps.can_channel_points);
    }

    #[test]
    fn limited_read_only_reason_non_empty() {
        let caps = PlatformCapabilities::limited_read_only("some reason");
        assert!(
            caps.limited_reason
                .as_deref()
                .is_some_and(|r| !r.is_empty()),
            "limited_reason must be non-empty when set"
        );
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
            limited_reason: Some("test reason".to_owned()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: PlatformCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn serde_omits_limited_reason_when_none() {
        let caps = PlatformCapabilities::chat_only();
        let json = serde_json::to_string(&caps).unwrap();
        assert!(
            !json.contains("limited_reason"),
            "limited_reason must be absent from JSON when None"
        );
    }

    #[test]
    fn serde_deserializes_legacy_json_without_limited_reason() {
        let legacy = r#"{"can_send_chat":true,"can_moderate":false,"can_subscribe_events":false,"can_polls":false,"can_predictions":false,"can_channel_points":false,"limited":false}"#;
        let caps: PlatformCapabilities = serde_json::from_str(legacy).unwrap();
        assert!(caps.limited_reason.is_none());
    }
}
