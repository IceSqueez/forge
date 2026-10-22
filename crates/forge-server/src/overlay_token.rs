use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use subtle::ConstantTimeEq as _;

use forge_storage::{
    CredentialId, CredentialsRepo, OVERLAY_TOKEN_PREFIX, SERVER_HTTP_OVERLAY_REQUIRE_TOKEN_DEFAULT,
    SERVER_OVERLAY_TOKEN_CREDENTIAL_ID, StorageError,
};

use crate::ServerError;
use crate::auth::mask_with_prefix;

const OVERLAY_TOKEN_BYTE_LEN: usize = 32;

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

    pub(crate) async fn abandon_rotation(
        &self,
        creds: &dyn CredentialsRepo,
    ) -> Result<(), StorageError> {
        let id = CredentialId::new(SERVER_OVERLAY_TOKEN_CREDENTIAL_ID);
        creds.store(&id, &self.current()).await
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use forge_storage::credentials::MockCredentialsRepo;

    use super::*;

    type Slots = Arc<Mutex<HashMap<String, String>>>;

    fn slot_repo(stored: Option<&str>) -> (MockCredentialsRepo, Slots) {
        let slots: Slots = Arc::default();
        if let Some(stored) = stored {
            slots.lock().expect("lock").insert(
                SERVER_OVERLAY_TOKEN_CREDENTIAL_ID.to_owned(),
                stored.to_owned(),
            );
        }
        let mut creds = MockCredentialsRepo::new();
        let read = Arc::clone(&slots);
        creds
            .expect_load()
            .returning(move |id| Ok(read.lock().expect("lock").get(id.as_str()).cloned()));
        let write = Arc::clone(&slots);
        creds.expect_store().returning(move |id, value| {
            write
                .lock()
                .expect("lock")
                .insert(id.as_str().to_owned(), value.to_owned());
            Ok(())
        });
        (creds, slots)
    }

    fn persisted(slots: &Slots) -> Option<String> {
        slots
            .lock()
            .expect("lock")
            .get(SERVER_OVERLAY_TOKEN_CREDENTIAL_ID)
            .cloned()
    }

    #[test]
    fn a_generated_overlay_token_is_prefixed_and_carries_the_full_entropy_url_safe() {
        let token = generate_overlay_token();

        let body = token.strip_prefix(OVERLAY_TOKEN_PREFIX).expect("prefix");
        let decoded = URL_SAFE_NO_PAD.decode(body).expect("url-safe no-pad body");
        assert_eq!(decoded.len(), OVERLAY_TOKEN_BYTE_LEN);
        assert_ne!(token, generate_overlay_token());
    }

    #[tokio::test]
    async fn load_with_nothing_stored_issues_persists_and_requires_a_token() {
        let (creds, slots) = slot_repo(None);

        let state = OverlayTokenState::load(&creds).await.expect("load");

        let stored = persisted(&slots).expect("issued token persisted");
        assert!(stored.starts_with(OVERLAY_TOKEN_PREFIX));
        assert!(state.verify(&stored));
        assert!(
            state.required(),
            "an unset setting must leave overlay files gated"
        );
    }

    #[tokio::test]
    async fn load_keeps_a_stored_overlay_token() {
        let (creds, _slots) = slot_repo(Some("fgo_already-issued"));

        let state = OverlayTokenState::load(&creds).await.expect("load");

        assert_eq!(state.current(), "fgo_already-issued");
    }

    #[tokio::test]
    async fn load_replaces_a_stored_value_that_is_not_an_overlay_token() {
        for foreign in ["fg_bearer-shaped", "unprefixed", ""] {
            let (creds, slots) = slot_repo(Some(foreign));

            let state = OverlayTokenState::load(&creds).await.expect("load");

            let stored = persisted(&slots).expect("replacement persisted");
            assert!(
                stored.starts_with(OVERLAY_TOKEN_PREFIX),
                "stored {foreign:?}"
            );
            assert!(
                !state.verify(foreign),
                "stored {foreign:?} still opens overlay files"
            );
        }
    }

    #[tokio::test]
    async fn load_treats_an_undecryptable_overlay_token_as_absent_and_issues_a_new_one() {
        let mut creds = MockCredentialsRepo::new();
        creds
            .expect_load()
            .returning(|_| Err(StorageError::Decryption));
        let issued = Arc::new(Mutex::new(None));
        let write = Arc::clone(&issued);
        creds.expect_store().times(1).returning(move |_, value| {
            *write.lock().expect("lock") = Some(value.to_owned());
            Ok(())
        });

        let state = OverlayTokenState::load(&creds).await.expect("load");

        let token = issued.lock().expect("lock").clone().expect("issued");
        assert!(state.verify(&token));
    }

    #[tokio::test]
    async fn load_propagates_a_non_decryption_storage_error_and_issues_nothing() {
        let mut creds = MockCredentialsRepo::new();
        creds.expect_load().returning(|_| {
            Err(StorageError::Connection {
                reason: "pool exhausted".into(),
            })
        });
        creds.expect_store().times(0);

        assert!(OverlayTokenState::load(&creds).await.is_err());
    }

    #[test]
    fn verify_admits_only_the_exact_token() {
        let state = OverlayTokenState::with_token("fgo_secretvalue".to_owned(), true);

        assert!(state.verify("fgo_secretvalue"));
        for wrong in [
            "",
            "fgo_",
            "fgo_secretvalu",
            "fgo_secretvalue0",
            "FGO_SECRETVALUE",
            "fgo_secretvalue ",
            "fgo_secretvaluf",
        ] {
            assert!(!state.verify(wrong), "candidate {wrong:?}");
        }
    }

    #[test]
    fn a_masked_overlay_token_keeps_its_prefix_and_tail_but_never_the_body() {
        let token = generate_overlay_token();
        let body = token.strip_prefix(OVERLAY_TOKEN_PREFIX).expect("prefix");

        let masked = masked_overlay_token(&token);

        assert!(
            masked.starts_with(OVERLAY_TOKEN_PREFIX),
            "masked = {masked}"
        );
        assert!(
            masked.ends_with(&token[token.len() - 4..]),
            "masked = {masked}"
        );
        assert!(
            !masked.contains(&body[..body.len() - 4]),
            "masked = {masked}"
        );
    }
}
