use forge_server::{ServerError, ServerHandle};

pub(crate) async fn restart_ignoring_disabled(handle: &ServerHandle) -> Result<(), String> {
    match handle.restart().await {
        Ok(()) => Ok(()),
        Err(ServerError::Disabled) => Ok(()),
        Err(other) => Err(other.to_string()),
    }
}
