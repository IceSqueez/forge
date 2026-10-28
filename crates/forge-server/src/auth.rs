use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use subtle::ConstantTimeEq as _;
use tokio::sync::{RwLock, watch};

use forge_storage::{CredentialId, CredentialsRepo, StorageError};

use crate::ServerError;
use crate::overlay_token::OverlayTokenState;

const TOKEN_BYTE_LEN: usize = 64;
const TOKEN_PREFIX: &str = "fg_";

pub struct AuthState {
    bearer_token: Arc<RwLock<String>>,
    token_generation: AtomicU64,
    reads_required: AtomicBool,
    policy_changed: watch::Sender<()>,
    overlay_token: OverlayTokenState,
}

impl AuthState {
    pub async fn load(
        auth_required_for_reads: bool,
        creds: &dyn CredentialsRepo,
    ) -> Result<Arc<Self>, ServerError> {
        let id = CredentialId::new(forge_storage::SERVER_BEARER_CREDENTIAL_ID);
        let stored = match creds.load(&id).await {
            Ok(stored) => stored,
            Err(e) if e.stored_secret_unrecoverable() => {
                tracing::warn!(
                    "stored server token cannot be decrypted with the current credentials key; issuing a new token"
                );
                None
            }
            Err(e) => return Err(e.into()),
        };
        let token = match stored.filter(|t| t.starts_with(TOKEN_PREFIX)) {
            Some(t) => t,
            None => {
                tracing::info!("no usable prefixed server token stored; issuing a new token");
                let t = generate_token();
                creds.store(&id, &t).await?;
                t
            }
        };
        let overlay_token = OverlayTokenState::load(creds).await?;
        Ok(Arc::new(Self::with_tokens(
            token,
            overlay_token,
            auth_required_for_reads,
        )))
    }

    fn with_tokens(
        token: String,
        overlay_token: OverlayTokenState,
        auth_required_for_reads: bool,
    ) -> Self {
        let (policy_changed, _) = watch::channel(());
        Self {
            bearer_token: Arc::new(RwLock::new(token)),
            token_generation: AtomicU64::new(0),
            reads_required: AtomicBool::new(auth_required_for_reads),
            policy_changed,
            overlay_token,
        }
    }

    pub fn overlay_token(&self) -> &OverlayTokenState {
        &self.overlay_token
    }

    pub async fn regenerate(&self, creds: &dyn CredentialsRepo) -> Result<String, ServerError> {
        let new_token = generate_token();
        let id = CredentialId::new(forge_storage::SERVER_BEARER_CREDENTIAL_ID);
        creds.store(&id, &new_token).await?;
        {
            let mut current = self.bearer_token.write().await;
            *current = new_token.clone();
            self.token_generation.fetch_add(1, Ordering::SeqCst);
        }
        self.policy_changed.send_replace(());
        Ok(new_token)
    }

    pub(crate) async fn current_token(&self) -> String {
        self.bearer_token.read().await.clone()
    }

    pub async fn verify(&self, candidate: &str) -> bool {
        self.verify_generation(candidate).await.is_some()
    }

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

    pub fn set_reads_required(&self, required: bool) {
        let previous = self.reads_required.swap(required, Ordering::SeqCst);
        if previous != required {
            self.policy_changed.send_replace(());
        }
    }

    pub(crate) fn policy_changes(&self) -> watch::Receiver<()> {
        self.policy_changed.subscribe()
    }

    pub fn admits_session(&self, bearer_generation: Option<u64>, overlay_session: bool) -> bool {
        match bearer_generation {
            Some(generation) => generation == self.token_generation(),
            None => overlay_session || !self.reads_required(),
        }
    }

    #[cfg(test)]
    pub fn for_test(auth_required_for_reads: bool, token: impl Into<String>) -> Arc<Self> {
        Arc::new(Self::with_tokens(
            token.into(),
            OverlayTokenState::with_token(crate::overlay_token::generate_overlay_token(), false),
            auth_required_for_reads,
        ))
    }
}

impl From<StorageError> for ServerError {
    fn from(e: StorageError) -> Self {
        ServerError::Storage(e.to_string())
    }
}

const MASKED_TAIL_LEN: usize = 4;
const EMPTY_TOKEN_PLACEHOLDER: &str = "-";
const MASKED_BODY: &str = "•••••";

pub fn masked_token(token: &str) -> String {
    mask_with_prefix(TOKEN_PREFIX, token)
}

pub(crate) fn mask_with_prefix(prefix: &str, token: &str) -> String {
    if token.is_empty() {
        return EMPTY_TOKEN_PLACEHOLDER.to_owned();
    }
    let tail: String = token
        .chars()
        .rev()
        .take(MASKED_TAIL_LEN)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}{MASKED_BODY}{tail}")
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use forge_storage::credentials::MockCredentialsRepo;
    use forge_storage::{CredentialId, SERVER_BEARER_CREDENTIAL_ID, StorageError};

    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    use super::{AuthState, TOKEN_BYTE_LEN, TOKEN_PREFIX, generate_token, masked_token};

    const FIRST_TOKEN: &str = "fg_first-token";

    type Slot = Arc<Mutex<Option<String>>>;
    type WriteLog = Arc<Mutex<Vec<String>>>;
    type OtherSlots = Arc<Mutex<HashMap<String, String>>>;

    fn is_bearer(id: &CredentialId) -> bool {
        id.as_str() == SERVER_BEARER_CREDENTIAL_ID
    }

    fn slot_backed_repo(initial: Option<&str>) -> (MockCredentialsRepo, Slot, WriteLog) {
        let slot: Slot = Arc::new(Mutex::new(initial.map(str::to_owned)));
        let writes: WriteLog = Arc::new(Mutex::new(Vec::new()));
        let others: OtherSlots = Arc::new(Mutex::new(HashMap::new()));
        let mut creds = MockCredentialsRepo::new();
        let read = Arc::clone(&slot);
        let read_others = Arc::clone(&others);
        creds.expect_load().returning(move |id| {
            Ok(if is_bearer(id) {
                read.lock().expect("lock").clone()
            } else {
                read_others.lock().expect("lock").get(id.as_str()).cloned()
            })
        });
        let write = Arc::clone(&slot);
        let log = Arc::clone(&writes);
        creds.expect_store().returning(move |id, token| {
            if is_bearer(id) {
                *write.lock().expect("lock") = Some(token.to_owned());
                log.lock().expect("lock").push(token.to_owned());
            } else {
                others
                    .lock()
                    .expect("lock")
                    .insert(id.as_str().to_owned(), token.to_owned());
            }
            Ok(())
        });
        (creds, slot, writes)
    }

    #[test]
    fn generated_token_carries_the_prefix_and_a_url_safe_body_of_the_full_entropy_length() {
        let token = generate_token();

        let body = token.strip_prefix(TOKEN_PREFIX).expect("prefix");
        let decoded = URL_SAFE_NO_PAD.decode(body).expect("url-safe no-pad body");
        assert_eq!(decoded.len(), TOKEN_BYTE_LEN);
    }

    #[tokio::test]
    async fn load_keeps_a_stored_prefixed_token_and_writes_nothing() {
        let (creds, _slot, writes) = slot_backed_repo(Some("fg_already-issued"));

        let auth = AuthState::load(false, &creds).await.expect("load");

        assert!(auth.verify("fg_already-issued").await);
        assert!(writes.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn load_replaces_an_unprefixed_stored_token_and_rejects_the_old_value() {
        let (creds, slot, writes) = slot_backed_repo(Some("legacy-unprefixed"));

        let auth = AuthState::load(false, &creds).await.expect("load");

        let persisted = slot.lock().expect("lock").clone().expect("persisted");
        assert!(persisted.starts_with(TOKEN_PREFIX));
        assert_eq!(writes.lock().expect("lock").len(), 1);
        assert!(auth.verify(&persisted).await);
        assert!(!auth.verify("legacy-unprefixed").await);
    }

    #[tokio::test]
    async fn load_with_nothing_stored_mints_and_persists_a_prefixed_token() {
        let (creds, slot, writes) = slot_backed_repo(None);

        let auth = AuthState::load(false, &creds).await.expect("load");

        let persisted = slot.lock().expect("lock").clone().expect("persisted");
        assert!(persisted.starts_with(TOKEN_PREFIX));
        assert_eq!(writes.lock().expect("lock").len(), 1);
        assert!(auth.verify(&persisted).await);
    }

    #[tokio::test]
    async fn regenerated_token_is_prefixed_and_survives_a_fresh_load() {
        let (creds, _slot, writes) = slot_backed_repo(Some(FIRST_TOKEN));
        let auth = AuthState::load(false, &creds).await.expect("load");

        let regenerated = auth.regenerate(&creds).await.expect("regenerate");

        assert!(regenerated.starts_with(TOKEN_PREFIX));
        let reloaded = AuthState::load(false, &creds).await.expect("reload");
        assert!(reloaded.verify(&regenerated).await);
        assert!(!reloaded.verify(FIRST_TOKEN).await);
        assert_eq!(writes.lock().expect("lock").as_slice(), [regenerated]);
    }

    #[tokio::test]
    async fn load_treats_an_undecryptable_stored_bearer_as_absent_and_mints_a_working_token() {
        let mut creds = MockCredentialsRepo::new();
        creds.expect_load().returning(|id| {
            if is_bearer(id) {
                Err(StorageError::Decryption)
            } else {
                Ok(None)
            }
        });
        let minted = Arc::new(Mutex::new(None));
        let minted_write = Arc::clone(&minted);
        creds.expect_store().returning(move |id, token| {
            if is_bearer(id) {
                *minted_write.lock().expect("lock") = Some(token.to_owned());
            }
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

    #[test]
    fn masked_token_reveals_only_the_last_four_characters_and_never_the_body() {
        let masked = masked_token("fg_supersecretvalue9c4a");
        assert!(masked.ends_with("9c4a"), "masked = {masked}");
        assert!(!masked.contains("supersecret"), "masked = {masked}");
    }

    #[test]
    fn masked_generated_token_shows_the_prefix_and_last_four_but_never_the_body() {
        let token = generate_token();
        let body = token.strip_prefix(TOKEN_PREFIX).expect("prefix");

        let masked = masked_token(&token);

        assert!(masked.starts_with("fg_•••••"), "masked = {masked}");
        assert!(
            masked.ends_with(&token[token.len() - 4..]),
            "masked = {masked}"
        );
        assert!(
            !masked.contains(&body[..body.len() - 4]),
            "masked = {masked}"
        );
    }

    #[test]
    fn masked_token_keeps_tokens_not_longer_than_the_tail_whole() {
        for token in ["a", "abc", "abcd"] {
            let masked = masked_token(token);
            assert!(masked.ends_with(token), "token {token} masked to {masked}");
        }
    }

    #[test]
    fn masked_token_splits_multibyte_tokens_on_character_boundaries() {
        let masked = masked_token("префікс-тікт");
        assert!(masked.ends_with("тікт"), "masked = {masked}");
        assert!(!masked.contains("префікс"), "masked = {masked}");
    }

    #[test]
    fn masked_token_renders_a_placeholder_for_an_empty_token() {
        assert_eq!(masked_token(""), "-");
    }
}
