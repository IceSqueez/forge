use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, MutexGuard};

use super::pkce_flow::REFRESH_BUFFER as REFRESH_WINDOW;
use crate::credential_errors::reauth_required;
use crate::credential_state::{CredentialState, CredentialStateTracker};
use crate::error::PlatformError;

pub type PlatformFuture<T> = Pin<Box<dyn Future<Output = Result<T, PlatformError>> + Send>>;

pub type TokenSource = Arc<dyn Fn() -> PlatformFuture<String> + Send + Sync>;

const REFRESH_TASK_LOST: &str = "token refresh did not complete";
const REFRESH_BUFFER: Duration = Duration::seconds(REFRESH_WINDOW.as_secs().cast_signed());

pub fn expires_within_refresh_buffer(expires_at: OffsetDateTime) -> bool {
    expires_at <= OffsetDateTime::now_utc() + REFRESH_BUFFER
}

#[async_trait]
pub trait TokenRotation: Send + Sync + 'static {
    type Credential: Send + 'static;

    fn platform(&self) -> &'static str;

    async fn load(&self) -> Result<Option<Self::Credential>, PlatformError>;

    fn access_token<'a>(&self, credential: &'a Self::Credential) -> &'a str;

    fn near_expiry(&self, credential: &Self::Credential) -> bool;

    async fn rotate(&self, existing: Self::Credential) -> Result<Self::Credential, PlatformError>;

    fn task_lost(&self, _detail: String) -> PlatformError {
        PlatformError::Auth {
            reason: REFRESH_TASK_LOST.to_owned(),
        }
    }
}

enum RefreshTrigger {
    NearExpiry,
    Rejected(String),
}

pub struct SingleFlightRefresh<R> {
    rotation: Arc<R>,
    guard: Arc<Mutex<()>>,
    credential_state: CredentialStateTracker,
}

impl<R: TokenRotation> SingleFlightRefresh<R> {
    pub fn new(rotation: R) -> Self {
        Self {
            rotation: Arc::new(rotation),
            guard: Arc::new(Mutex::new(())),
            credential_state: CredentialStateTracker::new(),
        }
    }

    pub fn rotation(&self) -> &R {
        &self.rotation
    }

    pub fn credential_state(&self) -> &CredentialStateTracker {
        &self.credential_state
    }

    pub async fn exclusive(&self) -> MutexGuard<'_, ()> {
        self.guard.lock().await
    }

    pub async fn valid_credential(&self) -> Result<R::Credential, PlatformError> {
        let credential = require(self.rotation.as_ref()).await?;
        if !self.rotation.near_expiry(&credential) {
            return Ok(credential);
        }
        tracing::debug!(
            platform = self.rotation.platform(),
            "access token inside the refresh buffer; renewing"
        );
        self.detached_refresh(RefreshTrigger::NearExpiry).await
    }

    pub async fn replace_rejected(
        &self,
        failed_access_token: &str,
    ) -> Result<R::Credential, PlatformError> {
        self.detached_refresh(RefreshTrigger::Rejected(failed_access_token.to_owned()))
            .await
    }

    async fn detached_refresh(
        &self,
        trigger: RefreshTrigger,
    ) -> Result<R::Credential, PlatformError> {
        let rotation = Arc::clone(&self.rotation);
        let guard = Arc::clone(&self.guard);
        let credential_state = self.credential_state.clone();
        tokio::spawn(async move {
            let _guard = guard.lock().await;
            refresh_under_guard(rotation.as_ref(), trigger, &credential_state).await
        })
        .await
        .map_err(|e| self.rotation.task_lost(e.to_string()))?
    }
}

async fn require<R: TokenRotation>(rotation: &R) -> Result<R::Credential, PlatformError> {
    rotation
        .load()
        .await?
        .ok_or_else(|| reauth_required(rotation.platform()))
}

async fn refresh_under_guard<R: TokenRotation>(
    rotation: &R,
    trigger: RefreshTrigger,
    credential_state: &CredentialStateTracker,
) -> Result<R::Credential, PlatformError> {
    let existing = require(rotation).await?;
    let already_rotated = match &trigger {
        RefreshTrigger::NearExpiry => !rotation.near_expiry(&existing),
        RefreshTrigger::Rejected(failed) => rotation.access_token(&existing) != failed,
    };
    if already_rotated {
        tracing::debug!(
            platform = rotation.platform(),
            "token already rotated by a concurrent refresh; reusing it"
        );
        return Ok(existing);
    }
    let rotated = rotation.rotate(existing).await;
    match &rotated {
        Ok(_) => credential_state.report(CredentialState::Valid),
        Err(PlatformError::ReauthRequired { .. }) => {
            credential_state.report(CredentialState::ReauthRequired);
        }
        Err(_) => {}
    }
    rotated
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex as StdMutex, MutexGuard as StdMutexGuard};

    use super::*;

    #[derive(Clone, Debug)]
    struct Pair {
        access: String,
        stale: bool,
    }

    struct FakeRotation {
        stored: StdMutex<Option<Pair>>,
        rotations: AtomicUsize,
    }

    impl FakeRotation {
        fn holding(stored: Option<Pair>) -> Self {
            Self {
                stored: StdMutex::new(stored),
                rotations: AtomicUsize::new(0),
            }
        }

        fn stored(&self) -> StdMutexGuard<'_, Option<Pair>> {
            self.stored.lock().unwrap()
        }
    }

    #[async_trait]
    impl TokenRotation for FakeRotation {
        type Credential = Pair;

        fn platform(&self) -> &'static str {
            "fake"
        }

        async fn load(&self) -> Result<Option<Pair>, PlatformError> {
            Ok(self.stored().clone())
        }

        fn access_token<'a>(&self, credential: &'a Pair) -> &'a str {
            &credential.access
        }

        fn near_expiry(&self, credential: &Pair) -> bool {
            credential.stale
        }

        async fn rotate(&self, _existing: Pair) -> Result<Pair, PlatformError> {
            let round = self.rotations.fetch_add(1, Ordering::SeqCst) + 1;
            tokio::task::yield_now().await;
            let renewed = Pair {
                access: format!("rotated-{round}"),
                stale: false,
            };
            *self.stored() = Some(renewed.clone());
            Ok(renewed)
        }
    }

    struct FailingRotation {
        failure: fn() -> PlatformError,
    }

    #[async_trait]
    impl TokenRotation for FailingRotation {
        type Credential = Pair;

        fn platform(&self) -> &'static str {
            "fake"
        }

        async fn load(&self) -> Result<Option<Pair>, PlatformError> {
            Ok(pair("old", true))
        }

        fn access_token<'a>(&self, credential: &'a Pair) -> &'a str {
            &credential.access
        }

        fn near_expiry(&self, credential: &Pair) -> bool {
            credential.stale
        }

        async fn rotate(&self, _existing: Pair) -> Result<Pair, PlatformError> {
            Err((self.failure)())
        }
    }

    #[tokio::test]
    async fn a_failed_rotation_marks_reauth_only_when_the_grant_itself_was_refused() {
        let refused: fn() -> PlatformError = || reauth_required("fake");
        let transport: fn() -> PlatformError = || PlatformError::Network {
            reason: "offline".to_owned(),
        };
        for (failure, expected) in [
            (refused, CredentialState::ReauthRequired),
            (transport, CredentialState::Valid),
        ] {
            let refresh = SingleFlightRefresh::new(FailingRotation { failure });

            assert!(refresh.valid_credential().await.is_err());

            assert_eq!(refresh.credential_state().current(), expected);
        }
    }

    #[tokio::test]
    async fn a_successful_rotation_clears_a_reported_reauth() {
        let refresh = SingleFlightRefresh::new(FakeRotation::holding(pair("old", true)));
        refresh
            .credential_state()
            .report(CredentialState::ReauthRequired);

        refresh.valid_credential().await.unwrap();

        assert_eq!(refresh.credential_state().current(), CredentialState::Valid);
    }

    #[tokio::test]
    async fn a_signed_out_rejection_is_not_a_reauth() {
        let refresh = SingleFlightRefresh::new(FakeRotation::holding(None));

        assert!(refresh.replace_rejected("old").await.is_err());

        assert_eq!(refresh.credential_state().current(), CredentialState::Valid);
    }

    fn pair(access: &str, stale: bool) -> Option<Pair> {
        Some(Pair {
            access: access.to_owned(),
            stale,
        })
    }

    #[tokio::test]
    async fn concurrent_near_expiry_callers_rotate_once_and_share_the_new_token() {
        let refresh = Arc::new(SingleFlightRefresh::new(FakeRotation::holding(pair(
            "old", true,
        ))));

        let callers: Vec<_> = (0..8)
            .map(|_| {
                let refresh = Arc::clone(&refresh);
                tokio::spawn(async move { refresh.valid_credential().await })
            })
            .collect();
        for caller in callers {
            let credential = caller.await.unwrap().unwrap();
            assert_eq!(credential.access, "rotated-1");
        }

        assert_eq!(refresh.rotation().rotations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_fresh_token_is_returned_without_rotating() {
        let refresh = SingleFlightRefresh::new(FakeRotation::holding(pair("fresh", false)));

        let credential = refresh.valid_credential().await.unwrap();

        assert_eq!(credential.access, "fresh");
        assert_eq!(refresh.rotation().rotations.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn concurrent_rejections_of_one_token_rotate_once() {
        let refresh = Arc::new(SingleFlightRefresh::new(FakeRotation::holding(pair(
            "rejected", false,
        ))));

        let callers: Vec<_> = (0..8)
            .map(|_| {
                let refresh = Arc::clone(&refresh);
                tokio::spawn(async move { refresh.replace_rejected("rejected").await })
            })
            .collect();
        for caller in callers {
            assert_eq!(caller.await.unwrap().unwrap().access, "rotated-1");
        }

        assert_eq!(refresh.rotation().rotations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_rejection_of_an_already_replaced_token_reuses_the_stored_one() {
        let refresh = SingleFlightRefresh::new(FakeRotation::holding(pair("current", false)));

        let credential = refresh.replace_rejected("previous").await.unwrap();

        assert_eq!(credential.access, "current");
        assert_eq!(refresh.rotation().rotations.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_cancelled_caller_cannot_abort_a_rotation_in_flight() {
        let refresh = Arc::new(SingleFlightRefresh::new(FakeRotation::holding(pair(
            "old", true,
        ))));
        let hold = refresh.exclusive().await;

        let caller = {
            let refresh = Arc::clone(&refresh);
            tokio::spawn(async move { refresh.valid_credential().await })
        };
        tokio::task::yield_now().await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        drop(hold);

        let credential = refresh.valid_credential().await.unwrap();
        assert_eq!(credential.access, "rotated-1");
        assert_eq!(refresh.rotation().rotations.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_missing_credential_asks_for_reauthorization() {
        let refresh = SingleFlightRefresh::new(FakeRotation::holding(None));

        for result in [
            refresh.valid_credential().await,
            refresh.replace_rejected("any").await,
        ] {
            assert!(matches!(
                result,
                Err(PlatformError::ReauthRequired { platform }) if platform == "fake"
            ));
        }
    }
}
