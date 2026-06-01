use forge_platform_core::PlatformCapabilities;

/// Disclaimer surfaced in UI: Kick OAuth covers chat-write but chat-receive still
/// flows through the unofficial Pusher WebSocket — this hybrid posture is unique to Kick.
pub const KICK_COMMUNITY_NOTE: &str = "Chat receive uses the unofficial Pusher WebSocket — Kick exposes no official chat:read \
     scope. Chat send uses the official OAuth API. Not affiliated with Kick.com.";

pub fn kick_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_not_marked_limited() {
        let caps = kick_capabilities();
        assert!(!caps.limited);
        assert!(caps.limited_reason.is_none());
    }

    #[test]
    fn capabilities_enable_chat_send() {
        let caps = kick_capabilities();
        assert!(caps.can_send_chat);
    }

    #[test]
    fn unsupported_features_remain_false() {
        let caps = kick_capabilities();
        assert!(!caps.can_moderate);
        assert!(!caps.can_subscribe_events);
        assert!(!caps.can_polls);
        assert!(!caps.can_predictions);
        assert!(!caps.can_channel_points);
    }

    #[test]
    fn community_note_is_non_empty() {
        assert!(!KICK_COMMUNITY_NOTE.is_empty());
    }
}
