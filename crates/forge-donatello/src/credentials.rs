use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::Secret;
use serde::{Deserialize, Serialize};

use crate::error::DonatelloError;
use crate::token::parse_token;

pub const DONATELLO_CREDENTIAL_ID: &str = "donatello:default";

#[derive(Serialize, Deserialize)]
struct StoredToken {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    token: Secret,
}

fn credential_id() -> CredentialId {
    CredentialId::new(DONATELLO_CREDENTIAL_ID)
}

fn storage_failure(error: impl ToString) -> DonatelloError {
    DonatelloError::Credential {
        reason: error.to_string(),
    }
}

pub(crate) async fn load(creds: &dyn CredentialsRepo) -> Result<Option<Secret>, DonatelloError> {
    let Some(bundle) = creds
        .load(&credential_id())
        .await
        .map_err(storage_failure)?
    else {
        return Ok(None);
    };
    let stored: StoredToken = serde_json::from_str(&bundle)
        .map_err(|error| storage_failure(format!("{:?} error", error.classify())))?;
    parse_token(stored.token.expose()).map(Some)
}

pub(crate) async fn store(
    creds: &dyn CredentialsRepo,
    token: &Secret,
) -> Result<(), DonatelloError> {
    let bundle = serde_json::to_string(&StoredToken {
        token: token.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAVED_BUNDLE: &str = r#"{"token":"dn-saved-token"}"#;

    #[test]
    fn a_saved_token_loads_and_saves_back_byte_identical() {
        let loaded: StoredToken = serde_json::from_str(SAVED_BUNDLE).unwrap();

        assert_eq!(loaded.token.expose(), "dn-saved-token");
        assert_eq!(serde_json::to_string(&loaded).unwrap(), SAVED_BUNDLE);
    }
}
