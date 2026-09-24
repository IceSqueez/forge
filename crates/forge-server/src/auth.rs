use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use subtle::ConstantTimeEq as _;
use tokio::sync::{RwLock, watch};

use forge_storage::{CredentialId, CredentialsRepo, StorageError};

use crate::ServerError;

const BEARER_CREDENTIAL_ID: &str = "server:bearer";
const TOKEN_BYTE_LEN: usize = 64;

pub struct AuthState {
    bearer_token: Arc<RwLock<String>>,
    /// Bumped only while `bearer_token`'s write lock is held, so a read under the read lock
    /// always pairs a token with the generation that minted it.
    token_generation: AtomicU64,
    reads_required: AtomicBool,
    policy_changed: watch::Sender<()>,
}

impl AuthState {
    pub async fn load(
        auth_required_for_reads: bool,
        creds: &dyn CredentialsRepo,
    ) -> Result<Arc<Self>, ServerError> {
        let id = CredentialId::new(BEARER_CREDENTIAL_ID);
        let stored = match creds.load(&id).await {
            Ok(stored) => stored,
            Err(StorageError::Decryption) => {
                tracing::warn!(
                    "stored server token cannot be decrypted with the current credentials key; issuing a new token"
                );
                None
            }
            Err(e) => return Err(e.into()),
        };
        let token = match stored {
            Some(t) => t,
            None => {
                let t = generate_token();
                creds.store(&id, &t).await?;
                t
            }
        };
        Ok(Arc::new(Self::with_token(token, auth_required_for_reads)))
    }

    fn with_token(token: String, auth_required_for_reads: bool) -> Self {
        let (policy_changed, _) = watch::channel(());
        Self {
            bearer_token: Arc::new(RwLock::new(token)),
            token_generation: AtomicU64::new(0),
            reads_required: AtomicBool::new(auth_required_for_reads),
            policy_changed,
        }
    }

    /// The new token is handed out once and cannot be read back; every WebSocket session that
    /// authenticated with the previous token loses its authentication and is closed.
    pub async fn regenerate(&self, creds: &dyn CredentialsRepo) -> Result<String, ServerError> {
        let new_token = generate_token();
        let id = CredentialId::new(BEARER_CREDENTIAL_ID);
        creds.store(&id, &new_token).await?;
        {
            let mut current = self.bearer_token.write().await;
            *current = new_token.clone();
            self.token_generation.fetch_add(1, Ordering::SeqCst);
        }
        self.policy_changed.send_replace(());
        Ok(new_token)
    }

    pub async fn verify(&self, candidate: &str) -> bool {
        self.verify_generation(candidate).await.is_some()
    }

    /// The generation of the token `candidate` matched, for a session to record at auth time.
    pub(crate) async fn verify_generation(&self, candidate: &str) -> Option<u64> {
        let current = self.bearer_token.read().await;
        let generation = self.token_generation.load(Ordering::SeqCst);
        bool::from(current.as_bytes().ct_eq(candidate.as_bytes())).then_some(generation)
    }

    pub fn token_generation(&self) -> u64 {
        self.token_generation.load(Ordering::SeqCst)
    }

    pub fn reads_required(&self) -> bool {
        self.reads_required.load(Ordering::SeqCst)
    }

    /// Applies to the next request on every live session; turning it on closes the WebSocket
    /// sessions that hold neither a bearer nor an overlay credential.
    pub fn set_reads_required(&self, required: bool) {
        let previous = self.reads_required.swap(required, Ordering::SeqCst);
        if previous != required {
            self.policy_changed.send_replace(());
        }
    }

    pub(crate) fn policy_changes(&self) -> watch::Receiver<()> {
        self.policy_changed.subscribe()
    }

    /// Whether a live WebSocket session may stay open under the current token and read policy.
    /// `bearer_generation` is the generation recorded when the session presented the bearer.
    pub fn admits_session(&self, bearer_generation: Option<u64>, overlay_session: bool) -> bool {
        match bearer_generation {
            Some(generation) => generation == self.token_generation(),
            None => overlay_session || !self.reads_required(),
        }
    }

    #[cfg(test)]
    pub fn for_test(auth_required_for_reads: bool, token: impl Into<String>) -> Arc<Self> {
        Arc::new(Self::with_token(token.into(), auth_required_for_reads))
    }
}

impl From<StorageError> for ServerError {
    fn from(e: StorageError) -> Self {
        ServerError::Storage(e.to_string())
    }
}

fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use forge_storage::StorageError;
    use forge_storage::credentials::MockCredentialsRepo;

    use super::AuthState;

    const FIRST_TOKEN: &str = "first-token";

    #[tokio::test]
    async fn load_treats_an_undecryptable_stored_bearer_as_absent_and_mints_a_working_token() {
        let mut creds = MockCredentialsRepo::new();
        creds
            .expect_load()
            .returning(|_| Err(StorageError::Decryption));
        let minted = Arc::new(Mutex::new(None));
        let minted_write = Arc::clone(&minted);
        creds.expect_store().times(1).returning(move |_, token| {
            *minted_write.lock().expect("lock") = Some(token.to_owned());
            Ok(())
        });

        let auth = AuthState::load(false, &creds).await.expect("load");

        let token = minted.lock().expect("lock").clone().expect("store called");
        assert!(auth.verify(&token).await);
    }

    #[tokio::test]
    async fn load_propagates_a_non_decryption_storage_error_and_stores_nothing() {
        let mut creds = MockCredentialsRepo::new();
        creds.expect_load().returning(|_| {
            Err(StorageError::Connection {
                reason: "pool exhausted".into(),
            })
        });
        creds.expect_store().times(0);

        let result = AuthState::load(false, &creds).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn a_live_session_stays_only_on_the_current_token_or_while_its_class_is_still_admitted() {
        let auth = AuthState::for_test(false, FIRST_TOKEN);
        let stale = auth.token_generation();
        let mut creds = MockCredentialsRepo::new();
        creds.expect_store().returning(|_, _| Ok(()));
        auth.regenerate(&creds).await.expect("regenerate");
        let current = auth.token_generation();

        for (reads_required, bearer, overlay, admitted, case) in [
            (
                false,
                Some(current),
                false,
                true,
                "bearer on the current token",
            ),
            (
                true,
                Some(current),
                false,
                true,
                "bearer on the current token, reads closed",
            ),
            (
                false,
                Some(stale),
                false,
                false,
                "bearer on a rotated token",
            ),
            (
                false,
                Some(stale),
                true,
                false,
                "rotated bearer that also opened an overlay channel",
            ),
            (
                false,
                None,
                false,
                true,
                "anonymous reader while reads are open",
            ),
            (
                true,
                None,
                false,
                false,
                "anonymous reader once reads are closed",
            ),
            (true, None, true, true, "overlay page once reads are closed"),
        ] {
            auth.set_reads_required(reads_required);
            assert_eq!(auth.admits_session(bearer, overlay), admitted, "{case}");
        }
    }
}
