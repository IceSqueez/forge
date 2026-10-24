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
