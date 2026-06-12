use std::sync::Arc;

use forge_storage::CredentialsRepo;

use crate::credentials;
use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};

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

/// Resolves a Twitch login name to a numeric user_id via GET /helix/users.
///
/// Costs one Helix rate-limit token per call.
pub async fn resolve_user_id(
    transport: &dyn HelixTransport,
    login: &str,
) -> Result<String, HelixError> {
    let request = HelixRequest::new(HelixMethod::Get, "/helix/users").query("login", login);
    let resp = transport.execute(request).await?;
    resp["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|u| u["id"].as_str())
        .map(|s| s.to_owned())
        .ok_or_else(|| HelixError::Http {
            status: 404,
            body: format!("user not found: {login}"),
        })
}
