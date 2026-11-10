use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinControl, ControlFailure, PlatformError, credential_storage_error,
};
use forge_runtime::{FollowKnowledgeHandle, IntegrationSupervisor};
use forge_storage::{CredentialsRepo, StorageError};
use forge_types::{IntegrationId, PlatformId};
use futures_util::future::BoxFuture;

use crate::integrations::{BuiltinObject, ObsInstallSeed, VTubeInstallSeed};
use crate::platform_credentials::PlatformCredentialManagers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignOutOutcome {
    Disconnected,
    StillConnected,
}

async fn forget_then_disconnect<F, T, E>(
    forget: F,
    control: Option<Arc<dyn BuiltinControl>>,
) -> Result<SignOutOutcome, E>
where
    F: Future<Output = Result<T, E>>,
{
    forget.await?;
    let Some(control) = control else {
        return Ok(SignOutOutcome::Disconnected);
    };
    Ok(match control.disconnect().await {
        Ok(()) | Err(ControlFailure::NotConnected) => SignOutOutcome::Disconnected,
        Err(err) => {
            tracing::warn!(error = %err, "integration disconnect after sign-out failed");
            SignOutOutcome::StillConnected
        }
    })
}

type ForgetCredentials =
    Arc<dyn Fn() -> BoxFuture<'static, Result<(), PlatformError>> + Send + Sync>;
type ReleaseLiveObject = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
enum Teardown {
    DisconnectThenRelease(ReleaseLiveObject),
    RebuildSlot,
}

#[derive(Clone)]
pub struct SignOutRoute {
    forget: ForgetCredentials,
    teardown: Teardown,
}

impl SignOutRoute {
    pub fn platform(
        platform: PlatformId,
        managers: PlatformCredentialManagers,
        unmanaged: Arc<dyn CredentialsRepo>,
    ) -> Self {
        Self {
            forget: Arc::new(move || {
                let managers = managers.clone();
                let unmanaged = Arc::clone(&unmanaged);
                Box::pin(async move { managers.forget(platform, unmanaged.as_ref()).await })
            }),
            teardown: Teardown::RebuildSlot,
        }
    }

    pub fn obs(creds: Arc<dyn CredentialsRepo>, seed: ObsInstallSeed) -> Self {
        Self {
            forget: Arc::new(move || {
                let creds = Arc::clone(&creds);
                Box::pin(async move {
                    forge_obs::credentials::clear(creds.as_ref())
                        .await
                        .map(drop)
                        .map_err(|e| storage_failure(&forge_obs::OBS_INTEGRATION.id, e))
                })
            }),
            teardown: Teardown::DisconnectThenRelease(Arc::new(move || seed.clear())),
        }
    }

    pub fn vtube(creds: Arc<dyn CredentialsRepo>, seed: VTubeInstallSeed) -> Self {
        Self {
            forget: Arc::new(move || {
                let creds = Arc::clone(&creds);
                Box::pin(async move {
                    forge_vtube::credentials::clear(creds.as_ref())
                        .await
                        .map(drop)
                        .map_err(|e| storage_failure(&forge_vtube::VTUBE_INTEGRATION.id, e))
                })
            }),
            teardown: Teardown::DisconnectThenRelease(Arc::new(move || seed.clear())),
        }
    }
}

fn storage_failure(id: &IntegrationId, error: StorageError) -> PlatformError {
    credential_storage_error(id.as_str(), error.stored_secret_unrecoverable(), error)
}

#[async_trait]
pub trait SlotRebuild: Send + Sync {
    async fn rebuild(&self, id: &IntegrationId);
}

#[async_trait]
impl SlotRebuild for IntegrationSupervisor<BuiltinObject> {
    async fn rebuild(&self, id: &IntegrationId) {
        if let Some(slot) = self.slot(id) {
            slot.rebuild().await;
        }
    }
}

#[async_trait]
pub trait PlatformDataErasure: Send + Sync {
    async fn erase(&self, platform: PlatformId);
}

#[async_trait]
impl PlatformDataErasure for FollowKnowledgeHandle {
    async fn erase(&self, platform: PlatformId) {
        self.clear_platform(platform).await;
    }
}

#[derive(Default)]
pub struct SignOutRoutes {
    routes: HashMap<IntegrationId, SignOutRoute>,
}

impl SignOutRoutes {
    pub fn register(&mut self, id: IntegrationId, route: SignOutRoute) {
        self.routes.insert(id, route);
    }

    pub fn over(self, slots: Arc<dyn SlotRebuild>) -> IntegrationSignOut {
        IntegrationSignOut {
            routes: Arc::new(self.routes),
            slots,
            erasures: Arc::new(Vec::new()),
        }
    }
}

#[derive(Clone)]
pub struct IntegrationSignOut {
    routes: Arc<HashMap<IntegrationId, SignOutRoute>>,
    slots: Arc<dyn SlotRebuild>,
    erasures: Arc<Vec<Arc<dyn PlatformDataErasure>>>,
}

impl IntegrationSignOut {
    pub fn erasing(self, erasures: Vec<Arc<dyn PlatformDataErasure>>) -> Self {
        Self {
            erasures: Arc::new(erasures),
            ..self
        }
    }

    pub fn entry(&self, id: &IntegrationId) -> Option<SignOutEntry> {
        self.routes.get(id).map(|route| SignOutEntry {
            id: id.clone(),
            route: route.clone(),
            slots: Arc::clone(&self.slots),
            erasures: Arc::clone(&self.erasures),
        })
    }
}

pub struct SignOutEntry {
    id: IntegrationId,
    route: SignOutRoute,
    slots: Arc<dyn SlotRebuild>,
    erasures: Arc<Vec<Arc<dyn PlatformDataErasure>>>,
}

impl SignOutEntry {
    pub async fn run(
        self,
        control: Option<Arc<dyn BuiltinControl>>,
    ) -> Result<SignOutOutcome, PlatformError> {
        let Self {
            id,
            route,
            slots,
            erasures,
        } = self;
        let forget = (route.forget)();
        match route.teardown {
            Teardown::DisconnectThenRelease(release) => {
                let outcome = forget_then_disconnect(forget, control).await?;
                release();
                Ok(outcome)
            }
            Teardown::RebuildSlot => {
                let outcome = forget_then_disconnect(forget, None).await?;
                if let Some(platform) = PlatformId::from_canonical(id.as_str()) {
                    for erasure in erasures.iter() {
                        erasure.erase(platform).await;
                    }
                }
                slots.rebuild(&id).await;
                Ok(outcome)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_platform_core::ControlOutcome;

    use super::*;
    use crate::integrations::BuiltinRegistry;

    type Journal = Arc<Mutex<Vec<&'static str>>>;

    fn io_failure() -> PlatformError {
        PlatformError::Storage {
            reason: "credentials store is read-only".to_owned(),
        }
    }

    struct RecordingControl {
        disconnect_outcome: ControlOutcome,
        disconnects: AtomicUsize,
        journal: Journal,
    }

    impl RecordingControl {
        fn answering(disconnect_outcome: ControlOutcome, journal: &Journal) -> Arc<Self> {
            Arc::new(Self {
                disconnect_outcome,
                disconnects: AtomicUsize::new(0),
                journal: Arc::clone(journal),
            })
        }
    }

    #[async_trait]
    impl BuiltinControl for RecordingControl {
        async fn reconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> ControlOutcome {
            self.disconnects.fetch_add(1, Ordering::SeqCst);
            self.journal.lock().unwrap().push("disconnect");
            self.disconnect_outcome.clone()
        }
        async fn refresh_token(&self) -> ControlOutcome {
            Ok(())
        }
    }

    struct RecordingSlots(Journal);

    #[async_trait]
    impl SlotRebuild for RecordingSlots {
        async fn rebuild(&self, id: &IntegrationId) {
            assert_eq!(id, &subject());
            self.0.lock().unwrap().push("rebuild");
        }
    }

    fn subject() -> IntegrationId {
        IntegrationId::from_static("subject")
    }

    fn scripted_forget(journal: &Journal, deleted: bool) -> ForgetCredentials {
        let journal = Arc::clone(journal);
        Arc::new(move || {
            journal.lock().unwrap().push("forget");
            Box::pin(async move { if deleted { Ok(()) } else { Err(io_failure()) } })
        })
    }

    fn entry_over(route: SignOutRoute, journal: &Journal) -> SignOutEntry {
        let mut routes = SignOutRoutes::default();
        routes.register(subject(), route);
        routes
            .over(Arc::new(RecordingSlots(Arc::clone(journal))))
            .entry(&subject())
            .unwrap()
    }

    fn disconnecting_route(journal: &Journal, deleted: bool) -> SignOutRoute {
        let released = Arc::clone(journal);
        SignOutRoute {
            forget: scripted_forget(journal, deleted),
            teardown: Teardown::DisconnectThenRelease(Arc::new(move || {
                released.lock().unwrap().push("release");
            })),
        }
    }

    fn rebuilding_route(journal: &Journal, deleted: bool) -> SignOutRoute {
        SignOutRoute {
            forget: scripted_forget(journal, deleted),
            teardown: Teardown::RebuildSlot,
        }
    }

    #[tokio::test]
    async fn the_live_connection_is_dropped_only_after_the_credentials_are_forgotten() {
        for (deleted, disconnect_outcome, outcome, steps, case) in [
            (
                true,
                Ok(()),
                Some(SignOutOutcome::Disconnected),
                vec!["forget", "disconnect", "release"],
                "forgotten and disconnected",
            ),
            (
                true,
                Err(ControlFailure::NotConnected),
                Some(SignOutOutcome::Disconnected),
                vec!["forget", "disconnect", "release"],
                "forgotten, nothing was connected",
            ),
            (
                true,
                Err(ControlFailure::Transport),
                Some(SignOutOutcome::StillConnected),
                vec!["forget", "disconnect", "release"],
                "forgotten, disconnect failed",
            ),
            (
                true,
                Err(ControlFailure::Unauthorized),
                Some(SignOutOutcome::StillConnected),
                vec!["forget", "disconnect", "release"],
                "forgotten, disconnect refused",
            ),
            (
                false,
                Ok(()),
                None,
                vec!["forget"],
                "credentials could not be forgotten",
            ),
        ] {
            let journal = Journal::default();
            let control = RecordingControl::answering(disconnect_outcome, &journal);

            let result = entry_over(disconnecting_route(&journal, deleted), &journal)
                .run(Some(Arc::clone(&control) as Arc<dyn BuiltinControl>))
                .await;

            assert_eq!(
                (result.ok(), journal.lock().unwrap().clone()),
                (outcome, steps),
                "{case}"
            );
        }
    }

    #[tokio::test]
    async fn with_no_live_connection_a_forgotten_credential_counts_as_disconnected() {
        let journal = Journal::default();

        let result = entry_over(disconnecting_route(&journal, true), &journal)
            .run(None)
            .await;

        assert_eq!(result.ok(), Some(SignOutOutcome::Disconnected));
    }

    #[tokio::test]
    async fn a_rebuilt_slot_retires_the_connection_so_the_page_control_is_never_called() {
        for (deleted, outcome, steps) in [
            (
                true,
                Some(SignOutOutcome::Disconnected),
                vec!["forget", "rebuild"],
            ),
            (false, None, vec!["forget"]),
        ] {
            let journal = Journal::default();
            let control = RecordingControl::answering(Ok(()), &journal);

            let result = entry_over(rebuilding_route(&journal, deleted), &journal)
                .run(Some(Arc::clone(&control) as Arc<dyn BuiltinControl>))
                .await;

            assert_eq!(
                (
                    result.ok(),
                    journal.lock().unwrap().clone(),
                    control.disconnects.load(Ordering::SeqCst)
                ),
                (outcome, steps, 0),
                "deleted={deleted}"
            );
        }
    }

    #[test]
    fn an_integration_without_a_route_has_no_entry() {
        let journal = Journal::default();
        let mut routes = SignOutRoutes::default();
        routes.register(subject(), rebuilding_route(&journal, true));
        let sign_out = routes.over(Arc::new(RecordingSlots(journal)));

        assert!(sign_out.entry(&subject()).is_some());
        assert!(sign_out.entry(&IntegrationId::new("donatello")).is_none());
    }

    struct RefusingDeletes;

    #[async_trait]
    impl CredentialsRepo for RefusingDeletes {
        async fn store(
            &self,
            _: &forge_storage::CredentialId,
            _: &str,
        ) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &forge_storage::CredentialId) -> Result<bool, StorageError> {
            Err(StorageError::Connection {
                reason: "credentials store is read-only".to_owned(),
            })
        }
        async fn list_ids(&self) -> Result<Vec<forge_storage::CredentialId>, StorageError> {
            Ok(Vec::new())
        }
        async fn last_refresh(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<Option<time::OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn mark_refreshed(
            &self,
            _: &forge_storage::CredentialId,
        ) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn registry_holds(builtins: &BuiltinRegistry, id: &IntegrationId) -> bool {
        builtins.get(id).is_some()
    }

    async fn listening_port() -> (tokio::net::TcpListener, u16) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[tokio::test]
    async fn a_failed_credential_delete_leaves_the_obs_object_installed() {
        let (_listener, port) = listening_port().await;
        let builtins = BuiltinRegistry::default();
        let seed = ObsInstallSeed::new(forge_obs::SwitchableObsSink::new(), builtins.clone());
        let client = forge_obs::ObsClient::connect(
            &forge_obs::ObsEndpoint::parse(&format!("127.0.0.1:{port}"), "").unwrap(),
            Arc::new(forge_events::test_support::NullPublisher),
        )
        .await
        .unwrap();
        client.set_auto_reconnect(false);
        seed.install(Arc::new(client));
        let journal = Journal::default();
        let control = RecordingControl::answering(Ok(()), &journal);

        let result = entry_over(
            SignOutRoute::obs(Arc::new(RefusingDeletes), seed.clone()),
            &journal,
        )
        .run(Some(Arc::clone(&control) as Arc<dyn BuiltinControl>))
        .await;

        assert!(
            matches!(result, Err(PlatformError::Storage { .. })),
            "{result:?}"
        );
        assert_eq!(
            (
                registry_holds(&builtins, &forge_obs::OBS_INTEGRATION.id),
                seed.live().is_some(),
                control.disconnects.load(Ordering::SeqCst)
            ),
            (true, true, 0)
        );
    }

    #[tokio::test]
    async fn a_failed_platform_credential_delete_keeps_the_slot_running() {
        let journal = Journal::default();
        let route = SignOutRoute::platform(
            PlatformId::Kick,
            PlatformCredentialManagers::default(),
            Arc::new(RefusingDeletes),
        );

        let result = entry_over(route, &journal).run(None).await;

        assert!(
            matches!(result, Err(PlatformError::Storage { .. })),
            "{result:?}"
        );
        assert_eq!(journal.lock().unwrap().clone(), Vec::<&str>::new());
    }
}
