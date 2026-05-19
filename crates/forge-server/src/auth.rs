use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use subtle::ConstantTimeEq as _;
use tokio::sync::RwLock;

use forge_storage::{CredentialId, CredentialsRepo, StorageError};

use crate::ServerError;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";
const TOKEN_BYTE_LEN: usize = 64;

pub struct AuthState {
    bearer_token: Arc<RwLock<String>>,
    pub auth_required_for_reads: bool,
}

impl AuthState {
    pub async fn load(
        auth_required_for_reads: bool,
        creds: &dyn CredentialsRepo,
    ) -> Result<Arc<Self>, ServerError> {
        let id = CredentialId::new(BEARER_CREDENTIAL_ID);
        let token = match creds.load(&id).await? {
            Some(t) => t,
            None => {
                let t = generate_token();
                creds.store(&id, &t).await?;
                t
            }
        };
        Ok(Arc::new(Self {
            bearer_token: Arc::new(RwLock::new(token)),
            auth_required_for_reads,
        }))
    }

    /// Generates a fresh token, persists it, and rotates the in-memory value.
    /// Returns the new token so the caller can display it once.
    pub async fn regenerate(&self, creds: &dyn CredentialsRepo) -> Result<String, ServerError> {
        let new_token = generate_token();
        let id = CredentialId::new(BEARER_CREDENTIAL_ID);
        creds.store(&id, &new_token).await?;
        *self.bearer_token.write().await = new_token.clone();
        Ok(new_token)
    }

    pub async fn verify(&self, candidate: &str) -> bool {
        let current = self.bearer_token.read().await;
        bool::from(current.as_bytes().ct_eq(candidate.as_bytes()))
    }
}

impl From<StorageError> for ServerError {
    fn from(e: StorageError) -> Self {
        ServerError::Storage(e.to_string())
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
