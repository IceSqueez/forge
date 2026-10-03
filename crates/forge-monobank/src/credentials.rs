use forge_storage::{CredentialId, CredentialsRepo};
use serde::{Deserialize, Serialize};

use crate::error::MonobankError;
use crate::jar::JarId;
use crate::token::MonobankToken;

pub const MONOBANK_CREDENTIAL_ID: &str = "monobank:default";

#[derive(Serialize, Deserialize)]
struct StoredCredential {
    token: String,
    #[serde(default)]
    jar_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonobankCredential {
    pub(crate) token: MonobankToken,
    pub(crate) jar_id: Option<JarId>,
}

fn credential_id() -> CredentialId {
    CredentialId::new(MONOBANK_CREDENTIAL_ID)
}

fn storage_failure(error: impl ToString) -> MonobankError {
    MonobankError::Credential {
        reason: error.to_string(),
    }
}

pub(crate) async fn load(
    creds: &dyn CredentialsRepo,
) -> Result<Option<MonobankCredential>, MonobankError> {
    let Some(bundle) = creds
        .load(&credential_id())
        .await
        .map_err(storage_failure)?
    else {
        return Ok(None);
    };
    let stored: StoredCredential = serde_json::from_str(&bundle)
        .map_err(|error| storage_failure(format!("{:?} error", error.classify())))?;
    let token = MonobankToken::parse(&stored.token)?;
    let jar_id = stored.jar_id.as_deref().map(JarId::parse).transpose()?;
    Ok(Some(MonobankCredential { token, jar_id }))
}

pub(crate) async fn store(
    creds: &dyn CredentialsRepo,
    credential: &MonobankCredential,
) -> Result<(), MonobankError> {
    let bundle = serde_json::to_string(&StoredCredential {
        token: credential.token.expose().to_owned(),
        jar_id: credential
            .jar_id
            .as_ref()
            .map(|jar| jar.as_str().to_owned()),
    })
    .map_err(|error| storage_failure(format!("{:?} error", error.classify())))?;
    creds
        .store(&credential_id(), &bundle)
        .await
        .map_err(storage_failure)
}

pub(crate) async fn delete(creds: &dyn CredentialsRepo) -> Result<bool, MonobankError> {
    creds
        .delete(&credential_id())
        .await
        .map_err(storage_failure)
}
