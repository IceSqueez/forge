use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use forge_storage::{CredentialId, CredentialsRepo};
use rand::Rng as _;

use crate::ServerError;

pub(crate) fn mint_secret<const BYTE_LEN: usize>() -> String {
    let mut bytes = [0u8; BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) async fn load_or_issue(
    creds: &dyn CredentialsRepo,
    id: &CredentialId,
    prefix: &str,
    issue: fn() -> String,
) -> Result<String, ServerError> {
    let stored = match creds.load(id).await {
        Ok(stored) => stored,
        Err(e) if e.stored_secret_unrecoverable() => {
            tracing::warn!(
                prefix,
                "stored token cannot be decrypted with the current credentials key; issuing a new token"
            );
            None
        }
        Err(e) => return Err(e.into()),
    };
    if let Some(token) = stored.filter(|t| t.starts_with(prefix)) {
        return Ok(token);
    }
    tracing::info!(prefix, "no usable stored token; issuing a new token");
    let token = issue();
    creds.store(id, &token).await?;
    Ok(token)
}
