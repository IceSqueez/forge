use forge_storage::{CredentialId, CredentialsRepo};
use serde::{Deserialize, Serialize};

use crate::error::DonatelloError;
use crate::token::DonatelloToken;

pub const DONATELLO_CREDENTIAL_ID: &str = "donatello:default";

#[derive(Serialize, Deserialize)]
struct StoredToken {
    token: String,
}

fn credential_id() -> CredentialId {
    CredentialId::new(DONATELLO_CREDENTIAL_ID)
}

fn storage_failure(error: impl ToString) -> DonatelloError {
    DonatelloError::Credential {
        reason: error.to_string(),
    }
}

pub(crate) async fn load(
    creds: &dyn CredentialsRepo,
) -> Result<Option<DonatelloToken>, DonatelloError> {
    let Some(bundle) = creds
        .load(&credential_id())
        .await
        .map_err(storage_failure)?
    else {
        return Ok(None);
    };
    let stored: StoredToken = serde_json::from_str(&bundle)
        .map_err(|error| storage_failure(format!("{:?} error", error.classify())))?;
    DonatelloToken::parse(&stored.token).map(Some)
}

pub(crate) async fn store(
    creds: &dyn CredentialsRepo,
    token: &DonatelloToken,
) -> Result<(), DonatelloError> {
    let bundle = serde_json::to_string(&StoredToken {
        token: token.expose().to_owned(),
    })
    .map_err(|error| storage_failure(format!("{:?} error", error.classify())))?;
    creds
        .store(&credential_id(), &bundle)
        .await
        .map_err(storage_failure)
}

pub(crate) async fn delete(creds: &dyn CredentialsRepo) -> Result<bool, DonatelloError> {
    creds
        .delete(&credential_id())
        .await
        .map_err(storage_failure)
}
