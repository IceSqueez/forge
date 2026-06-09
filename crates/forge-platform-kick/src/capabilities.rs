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
