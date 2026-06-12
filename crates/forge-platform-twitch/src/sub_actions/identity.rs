use std::sync::Arc;

use forge_storage::CredentialsRepo;

use crate::credentials;
use crate::helix::HelixError;

pub struct SelfIdentity {
    creds: Arc<dyn CredentialsRepo>,
}

impl SelfIdentity {
    pub fn new(creds: Arc<dyn CredentialsRepo>) -> Self {
        Self { creds }
    }

    /// Loaded per call, so a re-auth under a different account takes effect
    /// without re-registering the runners.
    pub async fn user_id(&self) -> Result<String, HelixError> {
        let cred = credentials::load(self.creds.as_ref())
            .await
            .map_err(|e| HelixError::Credentials(e.to_string()))?
            .ok_or_else(|| HelixError::Credentials("no twitch credentials stored".to_owned()))?;
        Ok(cred.user_id)
    }
}
