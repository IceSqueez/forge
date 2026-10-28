use std::sync::Arc;

use forge_server::ServerHandle;
use forge_storage::{CredentialId, CredentialsRepo, SERVER_BEARER_CREDENTIAL_ID};

pub(crate) async fn current_bearer_token(
    server: Option<ServerHandle>,
    credentials: Arc<dyn CredentialsRepo>,
) -> Option<String> {
    match server {
        Some(handle) => Some(handle.bearer_token().await.into_inner()),
        None => credentials
            .load(&CredentialId::new(SERVER_BEARER_CREDENTIAL_ID))
            .await
            .ok()
            .flatten(),
    }
}
