use forge_platform_core::AuthFlow;

pub fn kick_auth_flow() -> AuthFlow {
    AuthFlow::None {
        reason: "Kick official event delivery is webhook-only; community Pusher \
                 WebSocket used for desktop real-time chat (read-only)"
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_variant() {
        assert!(matches!(kick_auth_flow(), AuthFlow::None { .. }));
    }

    #[test]
    fn reason_is_non_empty() {
        let flow = kick_auth_flow();
        let AuthFlow::None { reason } = flow else {
            unreachable!("kick_auth_flow always returns AuthFlow::None");
        };
        assert!(!reason.is_empty());
    }
}
