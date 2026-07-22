use forge_platform_core::PlatformCapabilities;

/// This hybrid posture (official write API, unofficial chat-receive WS) is unique to Kick.
pub const KICK_COMMUNITY_NOTE: &str = "Chat receive uses the unofficial Pusher WebSocket - Kick exposes no official chat:read \
     scope. Chat send uses the official OAuth API. Not affiliated with Kick.com.";

pub fn kick_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        can_send_chat: true,
        can_moderate: true,
        can_subscribe_events: false,
        can_polls: false,
        can_predictions: false,
        can_channel_points: true,
        limited: false,
        limited_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_reports_full_write_tier_not_limited() {
        let caps = kick_capabilities();
        assert!(caps.can_moderate, "moderation write tier must be enabled");
        assert!(
            caps.can_channel_points,
            "channel-points (rewards) tier must be enabled"
        );
        assert!(
            !caps.limited,
            "Kick must no longer report limited capability"
        );
    }
}
