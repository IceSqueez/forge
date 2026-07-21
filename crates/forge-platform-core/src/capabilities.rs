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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        let caps = PlatformCapabilities {
            can_send_chat: true,
            can_moderate: false,
            can_subscribe_events: false,
            can_polls: false,
            can_predictions: false,
            can_channel_points: false,
            limited: false,
            limited_reason: None,
        };
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
