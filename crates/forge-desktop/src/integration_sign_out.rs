use std::future::Future;
use std::sync::Arc;

use forge_platform_core::BuiltinControl;
use forge_storage::StorageError;

pub async fn forget_then_disconnect<F, T>(
    forget: F,
    control: Option<Arc<dyn BuiltinControl>>,
) -> Result<(), StorageError>
where
    F: Future<Output = Result<T, StorageError>>,
{
    forget.await?;
    if let Some(control) = control
        && let Err(err) = control.disconnect().await
    {
        tracing::warn!(error = %err, "integration disconnect after sign-out failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use forge_platform_core::{ControlFailure, ControlOutcome};

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
        for (forget_outcome, disconnect_outcome, signed_out, disconnects, case) in [
            (Ok(true), Ok(()), true, 1, "forgotten and disconnected"),
            (
                Ok(true),
                Err(ControlFailure::Transport),
                true,
                1,
                "forgotten, disconnect failed",
            ),
            (
                Err(store_unreachable()),
                Ok(()),
                false,
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
                (result.is_ok(), control.disconnects.load(Ordering::SeqCst)),
                (signed_out, disconnects),
                "{case}"
            );
        }
    }
}
