use std::future::Future;
use std::sync::Arc;

use forge_platform_core::{BuiltinControl, ControlFailure};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOutOutcome {
    Disconnected,
    StillConnected,
}

pub async fn forget_then_disconnect<F, T, E>(
    forget: F,
    control: Option<Arc<dyn BuiltinControl>>,
) -> Result<SignOutOutcome, E>
where
    F: Future<Output = Result<T, E>>,
{
    forget.await?;
    let Some(control) = control else {
        return Ok(SignOutOutcome::Disconnected);
    };
    Ok(match control.disconnect().await {
        Ok(()) | Err(ControlFailure::NotConnected) => SignOutOutcome::Disconnected,
        Err(err) => {
            tracing::warn!(error = %err, "integration disconnect after sign-out failed");
            SignOutOutcome::StillConnected
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use forge_platform_core::{ControlFailure, ControlOutcome};
    use forge_storage::StorageError;

    use super::*;
    use crate::test_support::runtime;

    struct RecordingControl {
        disconnect_outcome: ControlOutcome,
        disconnects: AtomicUsize,
    }

    #[async_trait]
    impl BuiltinControl for RecordingControl {
        async fn reconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> ControlOutcome {
            self.disconnects.fetch_add(1, Ordering::SeqCst);
            self.disconnect_outcome.clone()
        }
        async fn refresh_token(&self) -> ControlOutcome {
            Ok(())
        }
    }

    fn store_unreachable() -> StorageError {
        StorageError::Connection {
            reason: "credentials store unreachable".to_owned(),
        }
    }

    #[test]
    fn the_live_connection_is_dropped_only_after_the_credentials_are_forgotten() {
        let rt = runtime();
        for (forget_outcome, disconnect_outcome, outcome, disconnects, case) in [
            (
                Ok(true),
                Ok(()),
                Some(SignOutOutcome::Disconnected),
                1,
                "forgotten and disconnected",
            ),
            (
                Ok(true),
                Err(ControlFailure::NotConnected),
                Some(SignOutOutcome::Disconnected),
                1,
                "forgotten, nothing was connected",
            ),
            (
                Ok(true),
                Err(ControlFailure::Transport),
                Some(SignOutOutcome::StillConnected),
                1,
                "forgotten, disconnect failed",
            ),
            (
                Ok(true),
                Err(ControlFailure::Unauthorized),
                Some(SignOutOutcome::StillConnected),
                1,
                "forgotten, disconnect refused",
            ),
            (
                Err(store_unreachable()),
                Ok(()),
                None,
                0,
                "credentials could not be forgotten",
            ),
        ] {
            let control = Arc::new(RecordingControl {
                disconnect_outcome,
                disconnects: AtomicUsize::new(0),
            });

            let result = rt.block_on(forget_then_disconnect(
                async move { forget_outcome },
                Some(Arc::clone(&control) as Arc<dyn BuiltinControl>),
            ));

            assert_eq!(
                (result.ok(), control.disconnects.load(Ordering::SeqCst)),
                (outcome, disconnects),
                "{case}"
            );
        }
    }

    #[test]
    fn with_no_live_connection_a_forgotten_credential_counts_as_disconnected() {
        let rt = runtime();

        let result = rt.block_on(forget_then_disconnect(
            async { Ok::<bool, StorageError>(true) },
            None,
        ));

        assert_eq!(result.ok(), Some(SignOutOutcome::Disconnected));
    }
}
