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

    pub async fn user_id(&self) -> Result<String, HelixError> {
        Ok(self.stored().await?.user_id)
    }

    pub(crate) async fn stored(&self) -> Result<credentials::StoredCredential, HelixError> {
        credentials::load(self.creds.as_ref())
            .await
            .map_err(|e| HelixError::Credentials(e.to_string()))?
            .ok_or_else(|| HelixError::Credentials("no twitch credentials stored".to_owned()))
    }
}

pub(crate) fn login_handle(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed.strip_prefix('@').unwrap_or(trimmed).trim_start()
}

pub(crate) fn normalize_login(raw: &str) -> String {
    login_handle(raw).to_lowercase()
}

pub async fn resolve_user_id(
    transport: &dyn HelixTransport,
    login: &str,
) -> Result<String, HelixError> {
    let login = normalize_login(login);
    let request =
        HelixRequest::new(HelixMethod::Get, "/helix/users").query("login", login.as_str());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BroadcasterTier {
    #[default]
    Standard,
    Affiliate,
    Partner,
}

impl BroadcasterTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Affiliate => "Affiliate",
            Self::Partner => "Partner",
        }
    }

    pub(crate) fn from_helix_str(raw: &str) -> Self {
        match raw {
            "partner" => Self::Partner,
            "affiliate" => Self::Affiliate,
            _ => Self::Standard,
        }
    }
}

pub async fn resolve_broadcaster_tier(
    transport: &dyn HelixTransport,
    user_id: &str,
) -> Result<BroadcasterTier, HelixError> {
    let request = HelixRequest::new(HelixMethod::Get, "/helix/users").query("id", user_id);
    let resp = transport.execute(request).await?;
    let raw = resp["data"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|u| u["broadcaster_type"].as_str())
        .unwrap_or_default();
    Ok(BroadcasterTier::from_helix_str(raw))
}
