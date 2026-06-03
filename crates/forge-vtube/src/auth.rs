#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    Cold,
    AwaitingToken,
    Connected,
    AuthRequired,
}

#[derive(Debug, Clone)]
pub enum AuthEvent {
    AuthStarted,
    TokenObtained(String),
    TokenRejected,
    Disconnected,
}

pub struct AuthStateMachine {
    state: AuthState,
}

impl AuthStateMachine {
    pub fn new() -> Self {
        Self {
            state: AuthState::Cold,
        }
    }

    pub fn state(&self) -> AuthState {
        self.state
    }

    pub fn transition(&mut self, event: AuthEvent) -> AuthState {
        self.state = match (self.state, event) {
            (AuthState::Cold, AuthEvent::AuthStarted) => AuthState::AwaitingToken,
            (AuthState::Cold, AuthEvent::TokenObtained(_)) => AuthState::Connected,
            (AuthState::AwaitingToken, AuthEvent::TokenObtained(_)) => AuthState::Connected,
            (AuthState::AwaitingToken, AuthEvent::TokenRejected) => AuthState::Cold,
            (AuthState::AwaitingToken, AuthEvent::Disconnected) => AuthState::Cold,
            (AuthState::Connected, AuthEvent::TokenRejected) => AuthState::AuthRequired,
            (AuthState::Connected, AuthEvent::Disconnected) => AuthState::Cold,
            (AuthState::AuthRequired, AuthEvent::AuthStarted) => AuthState::AwaitingToken,
            (state, _) => state,
        };
        self.state
    }
}

impl Default for AuthStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn cold_transitions_to_awaiting_on_auth_started() {
        let mut sm = AuthStateMachine::new();
        assert_eq!(sm.state(), AuthState::Cold);
        let next = sm.transition(AuthEvent::AuthStarted);
        assert_eq!(next, AuthState::AwaitingToken);
        assert_eq!(sm.state(), AuthState::AwaitingToken);
    }

    #[test]
    fn awaiting_transitions_to_connected_on_token_obtained() {
        let mut sm = AuthStateMachine::new();
        sm.transition(AuthEvent::AuthStarted);
        let next = sm.transition(AuthEvent::TokenObtained("tok-xyz".into()));
        assert_eq!(next, AuthState::Connected);
        assert_eq!(sm.state(), AuthState::Connected);
    }

    #[test]
    fn connected_transitions_to_auth_required_on_token_rejected() {
        let mut sm = AuthStateMachine::new();
        sm.transition(AuthEvent::AuthStarted);
        sm.transition(AuthEvent::TokenObtained("tok-xyz".into()));
        let next = sm.transition(AuthEvent::TokenRejected);
        assert_eq!(next, AuthState::AuthRequired);
        assert_eq!(sm.state(), AuthState::AuthRequired);
    }

    #[test]
    fn reconnect_path_cold_to_connected_via_stored_token() {
        let mut sm = AuthStateMachine::new();
        let after_start = sm.transition(AuthEvent::AuthStarted);
        assert_eq!(after_start, AuthState::AwaitingToken);
        let after_token = sm.transition(AuthEvent::TokenObtained("stored-tok".into()));
        assert_eq!(after_token, AuthState::Connected);
    }

    #[test]
    fn disconnect_from_connected_resets_to_cold() {
        let mut sm = AuthStateMachine::new();
        sm.transition(AuthEvent::AuthStarted);
        sm.transition(AuthEvent::TokenObtained("tok".into()));
        assert_eq!(sm.state(), AuthState::Connected);
        let next = sm.transition(AuthEvent::Disconnected);
        assert_eq!(next, AuthState::Cold);
    }

    #[test]
    fn rejected_token_in_awaiting_resets_to_cold() {
        let mut sm = AuthStateMachine::new();
        sm.transition(AuthEvent::AuthStarted);
        let next = sm.transition(AuthEvent::TokenRejected);
        assert_eq!(next, AuthState::Cold);
        assert_eq!(sm.state(), AuthState::Cold);
    }
}
