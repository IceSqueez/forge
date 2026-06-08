use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use serde::{Deserialize, Serialize};

pub const VTUBE_CREDENTIAL_ID: &str = "vtube:default";

#[derive(Clone, Serialize, Deserialize)]
pub struct VTubeCredentials {
    pub token: String,
    pub api_version: String,
}

impl std::fmt::Debug for VTubeCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VTubeCredentials")
            .field("token", &"***")
            .field("api_version", &self.api_version)
            .finish()
    }
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    token: &str,
    api_version: &str,
) -> Result<(), StorageError> {
    let bundle = serde_json::json!({
        "token": token,
        "api_version": api_version,
    });
    creds
        .store(&CredentialId::new(VTUBE_CREDENTIAL_ID), &bundle.to_string())
        .await
}

pub async fn load(creds: &dyn CredentialsRepo) -> Result<Option<VTubeCredentials>, StorageError> {
    let Some(json) = creds.load(&CredentialId::new(VTUBE_CREDENTIAL_ID)).await? else {
        return Ok(None);
    };
    let parsed: VTubeCredentials = serde_json::from_str(&json)?;
    Ok(Some(parsed))
}

pub async fn clear(creds: &dyn CredentialsRepo) -> Result<bool, StorageError> {
    creds.delete(&CredentialId::new(VTUBE_CREDENTIAL_ID)).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credentials_serde_roundtrip() {
        let creds = VTubeCredentials {
            token: "tok-abc123".into(),
            api_version: "1.0".into(),
        };
        let json = serde_json::to_string(&creds).unwrap();
        let back: VTubeCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.token, creds.token);
        assert_eq!(back.api_version, creds.api_version);
    }

    #[test]
    fn debug_redacts_token() {
        let creds = VTubeCredentials {
            token: "super-secret-vtube-token".into(),
            api_version: "1.0".into(),
        };
        let s = format!("{creds:?}");
        assert!(!s.contains("super-secret-vtube-token"));
        assert!(s.contains("***"));
        assert!(s.contains("1.0"));
    }
}
