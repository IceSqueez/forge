use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use subtle::ConstantTimeEq as _;

use forge_storage::{
    CredentialId, CredentialsRepo, SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
    SERVER_OVERLAY_TOKEN_CREDENTIAL_ID, StorageError,
};

use crate::ServerError;
use crate::auth::mask_with_prefix;

const OVERLAY_TOKEN_BYTE_LEN: usize = 32;
const OVERLAY_TOKEN_PREFIX: &str = "fgo_";

pub struct OverlayTokenState {
    token: ArcSwap<String>,
    required: AtomicBool,
}

impl OverlayTokenState {
    pub(crate) async fn load(creds: &dyn CredentialsRepo) -> Result<Self, ServerError> {
        let id = CredentialId::new(SERVER_OVERLAY_TOKEN_CREDENTIAL_ID);
        let stored = match creds.load(&id).await {
            Ok(stored) => stored,
            Err(StorageError::Decryption) => {
                tracing::warn!(
                    "stored overlay token cannot be decrypted with the current credentials key; issuing a new token"
                );
                None
            }
            Err(e) => return Err(e.into()),
        };
        let token = match stored.filter(|t| t.starts_with(OVERLAY_TOKEN_PREFIX)) {
            Some(t) => t,
            None => {
                tracing::info!("no usable overlay token stored; issuing a new token");
                let t = generate_overlay_token();
                creds.store(&id, &t).await?;
                t
            }
        };
        Ok(Self::with_token(
            token,
            SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
        ))
    }

    pub(crate) fn with_token(token: String, required: bool) -> Self {
        Self {
            token: ArcSwap::from_pointee(token),
            required: AtomicBool::new(required),
        }
    }

    pub fn verify(&self, candidate: &str) -> bool {
        let current = self.token.load();
        bool::from(current.as_bytes().ct_eq(candidate.as_bytes()))
    }

    pub fn current(&self) -> String {
        self.token.load().as_ref().clone()
    }

    pub fn required(&self) -> bool {
        self.required.load(Ordering::SeqCst)
    }

    pub fn set_required(&self, required: bool) {
        self.required.store(required, Ordering::SeqCst);
    }

    pub(crate) async fn stage_rotation(
        &self,
        creds: &dyn CredentialsRepo,
    ) -> Result<String, ServerError> {
        let fresh = generate_overlay_token();
        let id = CredentialId::new(SERVER_OVERLAY_TOKEN_CREDENTIAL_ID);
        creds.store(&id, &fresh).await?;
        Ok(fresh)
    }

    pub(crate) async fn abandon_rotation(&self, creds: &dyn CredentialsRepo) {
        let id = CredentialId::new(SERVER_OVERLAY_TOKEN_CREDENTIAL_ID);
        let previous = self.current();
        if let Err(e) = creds.store(&id, &previous).await {
            tracing::error!(error = %e, "could not restore the previous overlay token after an aborted rotation");
        }
    }

    pub(crate) fn commit_rotation(&self, fresh: String) {
        self.token.store(Arc::new(fresh));
    }
}

pub fn generate_overlay_token() -> String {
    let mut bytes = [0u8; OVERLAY_TOKEN_BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    format!("{OVERLAY_TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub fn masked_overlay_token(token: &str) -> String {
    mask_with_prefix(OVERLAY_TOKEN_PREFIX, token)
}
