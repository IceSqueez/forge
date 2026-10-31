use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, DonationProvider,
    PlatformEndpoints, QuickActions, SectionIcon,
};
use forge_runtime::{
    DEFAULT_TEARDOWN_GRACE, DonationIngest, EventBus, IntegrationFactory, IntegrationStartError,
    RunningIntegration, TaskGroup,
};
use forge_storage::{CredentialsRepo, DataProvider, StorageError, has_credentials_for};
use forge_types::IntegrationId;

use crate::integrations::{BuiltinObject, creds_of, publisher};

const DONATION_ICON: &str = "coin";

pub(crate) trait DonationIntegration:
    DonationProvider + BuiltinStatus + BuiltinHealth + BuiltinContent + QuickActions + BuiltinControl
{
}

impl<P> DonationIntegration for P where
    P: DonationProvider
        + BuiltinStatus
        + BuiltinHealth
        + BuiltinContent
        + QuickActions
        + BuiltinControl
{
}

pub(crate) struct DonationFactory<P> {
    provider: Arc<P>,
    creds: Arc<dyn CredentialsRepo>,
    ingest: Arc<DonationIngest>,
}

impl<P> DonationFactory<P> {
    pub(crate) fn provider(&self) -> Arc<P> {
        Arc::clone(&self.provider)
    }
}

#[async_trait]
impl<P: DonationIntegration + 'static> IntegrationFactory<BuiltinObject> for DonationFactory<P> {
    fn id(&self) -> IntegrationId {
        DonationProvider::provider(self.provider.as_ref()).clone()
    }

    fn teardown_grace(&self) -> Duration {
        DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration<BuiltinObject>, IntegrationStartError> {
        if let Err(failure) = self.provider.reconnect().await {
            tracing::warn!(integration = %self.id(), failure = ?failure, "donation polling could not be resumed");
        }
        let stream = self.provider.donations();
        let ingest = Arc::clone(&self.ingest);
        let id = self.id();
        let mut tasks = TaskGroup::default();
        tasks.track(tokio::spawn(async move { ingest.drain(id, stream).await }));
        let provider = Arc::clone(&self.provider);
        let object = BuiltinObject {
            icon: SectionIcon::new(DONATION_ICON),
            status: provider.clone(),
            health: provider.clone(),
            content: provider.clone(),
            quick: provider.clone(),
            control: Some(provider),
            collections: None,
            option_source: None,
            obs_client: None,
            vtube_client: None,
            follow: None,
            ban_list: None,
        };
        Ok(RunningIntegration::idle()
            .with_object(object)
            .with_teardown(Box::pin(async move { tasks.abort_all() })))
    }
}

pub(crate) fn wire_donatello(
    backend: &Arc<dyn DataProvider>,
    endpoints: &PlatformEndpoints,
    ingest: &Arc<DonationIngest>,
    bus: &Arc<EventBus>,
) -> Option<DonationFactory<forge_donatello::DonatelloProvider>> {
    let creds = creds_of(backend);
    let config = forge_donatello::DonatelloConfig::new(endpoints);
    let gateway =
        forge_donatello::donatello_gateway(&config, forge_donatello::default_rate_limiter())
            .inspect_err(
                |error| tracing::warn!(error = %error, "donatello integration unavailable"),
            )
            .ok()?;
    let provider = forge_donatello::DonatelloProvider::new(config, Arc::clone(&creds), gateway);
    Some(DonationFactory {
        provider: Arc::new(provider.with_event_publisher(publisher(bus))),
        creds,
        ingest: Arc::clone(ingest),
    })
}

pub(crate) fn wire_monobank(
    backend: &Arc<dyn DataProvider>,
    endpoints: &PlatformEndpoints,
    ingest: &Arc<DonationIngest>,
    bus: &Arc<EventBus>,
) -> Option<DonationFactory<forge_monobank::MonobankProvider>> {
    let creds = creds_of(backend);
    let config = forge_monobank::MonobankConfig::new(endpoints);
    let gateways =
        forge_monobank::monobank_gateways(&config, forge_monobank::MonobankRateLimits::official())
            .inspect_err(|error| tracing::warn!(error = %error, "monobank integration unavailable"))
            .ok()?;
    let provider = forge_monobank::MonobankProvider::new(config, Arc::clone(&creds), gateways);
    Some(DonationFactory {
        provider: Arc::new(provider.with_event_publisher(publisher(bus))),
        creds,
        ingest: Arc::clone(ingest),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use forge_platform_core::{
        CapabilityFlags, ConnectionState, ControlOutcome, DetailSection, DonationStream,
        HeaderAction, HealthMetric, HealthMetricId, HealthStream, HealthValue, QuickAction,
    };
    use forge_storage::{MockDonationRepo, SettingsRepo};
    use forge_types::{Donation, MessageCode, Shared};
    use tokio::sync::mpsc;

    use super::*;
    use crate::test_support::test_backend;
    use forge_runtime::{IntegrationSupervisor, LifecycleState};

    const SETTLE: Duration = Duration::from_secs(30);

    struct FakeService {
        id: IntegrationId,
        subscriptions: AtomicUsize,
        feeds: Mutex<Vec<mpsc::Sender<Result<Donation, forge_platform_core::PlatformError>>>>,
    }

    impl FakeService {
        fn new() -> Self {
            Self {
                id: IntegrationId::new("donatello"),
                subscriptions: AtomicUsize::new(0),
                feeds: Mutex::new(Vec::new()),
            }
        }

        fn last_feed(&self) -> mpsc::Sender<Result<Donation, forge_platform_core::PlatformError>> {
            self.feeds
                .lock()
                .unwrap()
                .last()
                .cloned()
                .expect("a stream was handed out")
        }
    }

    impl DonationProvider for FakeService {
        fn provider(&self) -> &IntegrationId {
            &self.id
        }

        fn donations(&self) -> DonationStream {
            self.subscriptions.fetch_add(1, Ordering::SeqCst);
            let (tx, mut rx) = mpsc::channel(1);
            self.feeds.lock().unwrap().push(tx);
            Box::pin(futures_util::stream::poll_fn(move |cx| rx.poll_recv(cx)))
        }
    }

    impl BuiltinStatus for FakeService {
        fn id(&self) -> &IntegrationId {
            &self.id
        }
        fn display_name(&self) -> &str {
            "Fake"
        }
        fn version(&self) -> Option<String> {
            None
        }
        fn connection(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        fn uptime(&self) -> Option<Duration> {
            None
        }
        fn endpoint(&self) -> Option<forge_types::UserText> {
            None
        }
        fn capability_flags(&self) -> CapabilityFlags {
            CapabilityFlags {
                limited: false,
                label: None,
            }
        }
        fn header_actions(&self) -> Vec<HeaderAction> {
            Vec::new()
        }
    }

    const STUB_METRIC_IDS: [HealthMetricId; 4] = [
        HealthMetricId::new(MessageCode::from_static("stub", "metric0")),
        HealthMetricId::new(MessageCode::from_static("stub", "metric1")),
        HealthMetricId::new(MessageCode::from_static("stub", "metric2")),
        HealthMetricId::new(MessageCode::from_static("stub", "metric3")),
    ];

    impl BuiltinHealth for FakeService {
        fn metrics(&self) -> [HealthMetric; 4] {
            std::array::from_fn(|i| HealthMetric {
                id: STUB_METRIC_IDS[i].clone(),
                value: HealthValue::Text {
                    primary: forge_types::VerbatimText::new("").into(),
                    secondary: None,
                },
            })
        }
        fn stream(&self) -> HealthStream {
            Box::pin(futures_util::stream::pending())
        }
    }

    impl BuiltinContent for FakeService {
        fn sections(&self) -> Vec<DetailSection> {
            Vec::new()
        }
    }

    impl QuickActions for FakeService {
        fn actions(&self) -> Vec<QuickAction> {
            Vec::new()
        }
    }

    #[async_trait]
    impl BuiltinControl for FakeService {
        async fn reconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn disconnect(&self) -> ControlOutcome {
            Ok(())
        }
        async fn refresh_token(&self) -> ControlOutcome {
            Ok(())
        }
    }

    fn supervised(service: &Arc<FakeService>) -> IntegrationSupervisor<BuiltinObject> {
        let (backend, _writes) = test_backend();
        let mut ledger = MockDonationRepo::new();
        ledger.expect_has_donations_from().returning(|_| Ok(true));
        let ingest = Arc::new(DonationIngest::new(
            Arc::new(ledger),
            Arc::new(crate::test_support::SilentPublisher),
            Shared::new(String::new()),
        ));
        let factory = DonationFactory {
            provider: Arc::clone(service),
            creds: Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            ingest,
        };
        IntegrationSupervisor::launch(
            vec![Arc::new(factory) as Arc<dyn forge_runtime::IntegrationFactory<BuiltinObject>>],
            backend as Arc<dyn SettingsRepo>,
            forge_runtime::IntegrationGate::new(),
            Arc::new(crate::integrations::BuiltinRegistry::default()),
            forge_runtime::spawn_live_viewer_aggregator(),
        )
    }

    async fn switch(
        supervisor: &IntegrationSupervisor<BuiltinObject>,
        id: &IntegrationId,
        enabled: bool,
    ) {
        let state = supervisor
            .slot(id)
            .expect("the factory has a slot")
            .set_enabled(enabled)
            .await
            .expect("the switch persists");
        let expected = if enabled {
            LifecycleState::Running
        } else {
            LifecycleState::Disabled
        };
        assert_eq!(state, expected);
    }

    #[tokio::test]
    async fn each_start_subscribes_to_the_donation_feed_exactly_once() {
        let service = Arc::new(FakeService::new());
        let supervisor = supervised(&service);
        let id = service.id.clone();

        switch(&supervisor, &id, true).await;
        switch(&supervisor, &id, false).await;
        switch(&supervisor, &id, true).await;

        assert_eq!(service.subscriptions.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn switching_the_service_off_stops_draining_its_donation_feed() {
        let service = Arc::new(FakeService::new());
        let supervisor = supervised(&service);
        let id = service.id.clone();
        switch(&supervisor, &id, true).await;
        let feed = service.last_feed();

        switch(&supervisor, &id, false).await;

        assert!(
            tokio::time::timeout(SETTLE, feed.closed()).await.is_ok(),
            "the drain task still holds the donation feed"
        );
    }

    #[tokio::test]
    async fn a_running_service_keeps_draining_its_donation_feed() {
        let service = Arc::new(FakeService::new());
        let supervisor = supervised(&service);
        let id = service.id.clone();

        switch(&supervisor, &id, true).await;
        tokio::task::yield_now().await;

        assert!(!service.last_feed().is_closed());
    }
}
