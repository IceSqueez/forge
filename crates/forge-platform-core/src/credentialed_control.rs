use async_trait::async_trait;

use crate::builtin::{ControlFailure, ControlOutcome};
use crate::chat::{ChatPlatform, ConnectionState};
use crate::credential_state::{CredentialState, CredentialStateTracker};
use crate::error::PlatformError;

#[async_trait]
pub trait CredentialedChatBundle: Send + Sync {
    fn chat_platform(&self) -> &dyn ChatPlatform;

    fn credential_state(&self) -> &CredentialStateTracker;

    async fn has_stored_credentials(&self) -> Result<bool, PlatformError>;

    async fn refresh_if_stored(&self) -> Result<bool, PlatformError>;
}

pub async fn reconnect_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    let stored = bundle
        .has_stored_credentials()
        .await
        .map_err(credential_load_failure)?;
    if !stored {
        return Err(ControlFailure::NotConnected);
    }
    bundle
        .chat_platform()
        .connect()
        .await
        .map_err(connection_failure)
}

pub async fn disconnect_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    if matches!(
        bundle.chat_platform().connection_state(),
        ConnectionState::Disconnected
    ) {
        return Err(ControlFailure::NotConnected);
    }
    bundle
        .chat_platform()
        .disconnect()
        .await
        .map_err(connection_failure)
}

pub async fn refresh_credentialed(bundle: &dyn CredentialedChatBundle) -> ControlOutcome {
    let outcome = match bundle.refresh_if_stored().await {
        Ok(true) => Ok(()),
        Ok(false) => Err(ControlFailure::NotConnected),
        Err(error) => Err(refresh_failure(&error)),
    };
    if matches!(outcome, Err(ControlFailure::Unauthorized)) {
        bundle
            .credential_state()
            .report(CredentialState::ReauthRequired);
    }
    outcome
}

fn refresh_failure(error: &PlatformError) -> ControlFailure {
    match error {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        PlatformError::Http {
            status: forge_net::status::UNAUTHORIZED,
            ..
        } => ControlFailure::Unauthorized,
        _ => ControlFailure::Transport,
    }
}

fn credential_load_failure(error: PlatformError) -> ControlFailure {
    tracing::warn!(%error, "control could not load stored credentials");
    ControlFailure::Transport
}

fn connection_failure(error: PlatformError) -> ControlFailure {
    match error {
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. } => {
            ControlFailure::Unauthorized
        }
        _ => ControlFailure::Transport,
    }
}
#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{EventChannel, EventSubscription};

    use super::*;

    struct ScriptedPlatform {
        state: ConnectionState,
        connect_error: Mutex<Option<PlatformError>>,
        disconnect_error: Mutex<Option<PlatformError>>,
        connects: AtomicUsize,
        disconnects: AtomicUsize,
    }

    impl ScriptedPlatform {
        fn in_state(state: ConnectionState) -> Self {
            Self {
                state,
                connect_error: Mutex::new(None),
                disconnect_error: Mutex::new(None),
                connects: AtomicUsize::new(0),
                disconnects: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ChatPlatform for ScriptedPlatform {
        fn connection_state(&self) -> ConnectionState {
            self.state
        }

        async fn connect(&self) -> Result<(), PlatformError> {
            self.connects.fetch_add(1, Ordering::SeqCst);
            self.connect_error
                .lock()
                .unwrap()
                .take()
                .map_or(Ok(()), Err)
        }

        async fn disconnect(&self) -> Result<(), PlatformError> {
            self.disconnects.fetch_add(1, Ordering::SeqCst);
            self.disconnect_error
                .lock()
                .unwrap()
                .take()
                .map_or(Ok(()), Err)
        }

        async fn send_message(&self, _channel: &str, _text: &str) -> Result<(), PlatformError> {
            Ok(())
        }

        fn events(&self) -> EventSubscription {
            EventChannel::new(1).subscribe()
        }
    }

    struct ScriptedBundle {
        platform: ScriptedPlatform,
        tracker: CredentialStateTracker,
        stored: Result<bool, ()>,
        refresh: Mutex<Option<Result<bool, PlatformError>>>,
    }

    impl ScriptedBundle {
        fn new(state: ConnectionState) -> Self {
            Self {
                platform: ScriptedPlatform::in_state(state),
                tracker: CredentialStateTracker::new(),
                stored: Ok(true),
                refresh: Mutex::new(Some(Ok(true))),
            }
        }

        fn refreshing(self, outcome: Result<bool, PlatformError>) -> Self {
            *self.refresh.lock().unwrap() = Some(outcome);
            self
        }
    }

    #[async_trait]
    impl CredentialedChatBundle for ScriptedBundle {
        fn chat_platform(&self) -> &dyn ChatPlatform {
            &self.platform
        }

        fn credential_state(&self) -> &CredentialStateTracker {
            &self.tracker
        }

        async fn has_stored_credentials(&self) -> Result<bool, PlatformError> {
            self.stored.map_err(|()| PlatformError::Network {
                reason: "store unreachable".to_owned(),
            })
        }

        async fn refresh_if_stored(&self) -> Result<bool, PlatformError> {
            self.refresh.lock().unwrap().take().unwrap_or(Ok(true))
        }
    }

    fn reauth() -> PlatformError {
        PlatformError::ReauthRequired {
            platform: "test".to_owned(),
        }
    }

    fn auth() -> PlatformError {
        PlatformError::Auth {
            reason: String::new(),
        }
    }

    fn network() -> PlatformError {
        PlatformError::Network {
            reason: String::new(),
        }
    }

    fn http(status: u16) -> PlatformError {
        PlatformError::Http {
            status,
            body: String::new(),
        }
    }

    #[tokio::test]
    async fn reconnect_connects_when_credentials_are_stored() {
        let bundle = ScriptedBundle::new(ConnectionState::Disconnected);

        assert_eq!(reconnect_credentialed(&bundle).await, Ok(()));
        assert_eq!(bundle.platform.connects.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn reconnect_without_credentials_is_not_connected_and_never_dials() {
        let mut bundle = ScriptedBundle::new(ConnectionState::Disconnected);
        bundle.stored = Ok(false);

        assert_eq!(
            reconnect_credentialed(&bundle).await,
            Err(ControlFailure::NotConnected)
        );
        assert_eq!(bundle.platform.connects.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reconnect_with_an_unreadable_store_is_a_transport_failure() {
        let mut bundle = ScriptedBundle::new(ConnectionState::Disconnected);
        bundle.stored = Err(());

        assert_eq!(
            reconnect_credentialed(&bundle).await,
            Err(ControlFailure::Transport)
        );
        assert_eq!(bundle.platform.connects.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reconnect_maps_connect_errors_to_control_failures() {
        for (error, failure) in [
            (reauth(), ControlFailure::Unauthorized),
            (auth(), ControlFailure::Unauthorized),
            (network(), ControlFailure::Transport),
            (
                http(forge_net::status::UNAUTHORIZED),
                ControlFailure::Transport,
            ),
        ] {
            let bundle = ScriptedBundle::new(ConnectionState::Disconnected);
            *bundle.platform.connect_error.lock().unwrap() = Some(error);

            assert_eq!(reconnect_credentialed(&bundle).await, Err(failure));
        }
    }

    #[tokio::test]
    async fn disconnect_of_a_disconnected_platform_is_not_connected_and_does_nothing() {
        let bundle = ScriptedBundle::new(ConnectionState::Disconnected);

        assert_eq!(
            disconnect_credentialed(&bundle).await,
            Err(ControlFailure::NotConnected)
        );
        assert_eq!(bundle.platform.disconnects.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn disconnect_closes_every_state_except_disconnected() {
        for state in [
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Reconnecting,
        ] {
            let bundle = ScriptedBundle::new(state);

            assert_eq!(disconnect_credentialed(&bundle).await, Ok(()), "{state:?}");
            assert_eq!(bundle.platform.disconnects.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn disconnect_maps_platform_errors_to_control_failures() {
        for (error, failure) in [
            (reauth(), ControlFailure::Unauthorized),
            (network(), ControlFailure::Transport),
        ] {
            let bundle = ScriptedBundle::new(ConnectionState::Connected);
            *bundle.platform.disconnect_error.lock().unwrap() = Some(error);

            assert_eq!(disconnect_credentialed(&bundle).await, Err(failure));
        }
    }

    #[tokio::test]
    async fn refresh_success_leaves_the_credential_state_alone() {
        let bundle = ScriptedBundle::new(ConnectionState::Connected);

        assert_eq!(refresh_credentialed(&bundle).await, Ok(()));
        assert_eq!(bundle.tracker.current(), CredentialState::Valid);
    }

    #[tokio::test]
    async fn refresh_without_stored_credentials_is_not_connected() {
        let bundle = ScriptedBundle::new(ConnectionState::Connected).refreshing(Ok(false));

        assert_eq!(
            refresh_credentialed(&bundle).await,
            Err(ControlFailure::NotConnected)
        );
        assert_eq!(bundle.tracker.current(), CredentialState::Valid);
    }

    #[tokio::test]
    async fn a_refresh_the_provider_refuses_reports_reauth_required() {
        for error in [reauth(), auth(), http(forge_net::status::UNAUTHORIZED)] {
            let bundle = ScriptedBundle::new(ConnectionState::Connected).refreshing(Err(error));

            assert_eq!(
                refresh_credentialed(&bundle).await,
                Err(ControlFailure::Unauthorized)
            );
            assert_eq!(bundle.tracker.current(), CredentialState::ReauthRequired);
        }
    }

    #[tokio::test]
    async fn a_transient_refresh_failure_is_transport_and_keeps_the_credentials_valid() {
        let bundle = ScriptedBundle::new(ConnectionState::Connected)
            .refreshing(Err(http(forge_net::status::TOO_MANY_REQUESTS)));

        assert_eq!(
            refresh_credentialed(&bundle).await,
            Err(ControlFailure::Transport)
        );
        assert_eq!(bundle.tracker.current(), CredentialState::Valid);
    }
}
