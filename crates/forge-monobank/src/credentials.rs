use forge_storage::{CredentialId, CredentialsRepo};
use forge_types::Secret;
use serde::{Deserialize, Serialize};

use crate::error::MonobankError;
use crate::jar::JarId;
use crate::token::parse_token;

pub const MONOBANK_CREDENTIAL_ID: &str = "monobank:default";

#[derive(Serialize, Deserialize)]
struct StoredCredential {
    #[serde(serialize_with = "Secret::serialize_exposed")]
    token: Secret,
    #[serde(default)]
    jar_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonobankCredential {
    pub(crate) token: Secret,
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
    let token = parse_token(stored.token.expose())?;
    let jar_id = stored.jar_id.as_deref().map(JarId::parse).transpose()?;
    Ok(Some(MonobankCredential { token, jar_id }))
}

pub(crate) async fn store(
    creds: &dyn CredentialsRepo,
    credential: &MonobankCredential,
) -> Result<(), MonobankError> {
    let bundle = serde_json::to_string(&StoredCredential {
        token: credential.token.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_credential_loads_and_saves_back_byte_identical_with_and_without_a_jar() {
        for saved in [
            r#"{"token":"uMono-saved","jar_id":"jar-1"}"#,
            r#"{"token":"uMono-saved","jar_id":null}"#,
        ] {
            let loaded: StoredCredential = serde_json::from_str(saved).unwrap();

            assert_eq!(loaded.token.expose(), "uMono-saved");
            assert_eq!(serde_json::to_string(&loaded).unwrap(), saved);
        }
    }

    #[test]
    fn a_credential_saved_before_jars_existed_loads_without_one() {
        let loaded: StoredCredential = serde_json::from_str(r#"{"token":"uMono-saved"}"#).unwrap();

        assert_eq!(loaded.jar_id, None);
    }

    #[test]
    fn debug_of_a_loaded_credential_never_carries_the_token() {
        let credential = MonobankCredential {
            token: Secret::new("uMono-debug-sentinel"),
            jar_id: None,
        };

        let rendered = format!("{credential:?} {credential:#?}");

        assert!(!rendered.contains("uMono-debug-sentinel"), "{rendered}");
    }
}
