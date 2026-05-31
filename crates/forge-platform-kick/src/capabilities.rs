use forge_platform_core::PlatformCapabilities;

pub const KICK_LIMITED_REASON: &str = "Community implementation — read-only via unofficial WebSocket. \
     Not affiliated with Kick.com. May break without notice.";

pub fn kick_capabilities() -> PlatformCapabilities {
    PlatformCapabilities::limited_read_only(KICK_LIMITED_REASON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_limited() {
        let caps = kick_capabilities();
        assert!(caps.limited);
    }

    #[test]
    fn capabilities_have_non_empty_reason() {
        let caps = kick_capabilities();
        assert!(
            caps.limited_reason
                .as_deref()
                .is_some_and(|r| !r.is_empty()),
            "limited_reason must be non-empty"
        );
    }

    #[test]
    fn no_send_or_moderation_flags_set() {
        let caps = kick_capabilities();
        assert!(!caps.can_send_chat);
        assert!(!caps.can_moderate);
        assert!(!caps.can_subscribe_events);
        assert!(!caps.can_polls);
        assert!(!caps.can_predictions);
        assert!(!caps.can_channel_points);
    }

    #[test]
    fn reason_matches_constant() {
        let caps = kick_capabilities();
        assert_eq!(caps.limited_reason.as_deref(), Some(KICK_LIMITED_REASON));
    }
}
