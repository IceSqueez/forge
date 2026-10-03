use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, DonationProvider,
    PlatformEndpoints, QuickActions, SectionIcon,
};
use forge_runtime::DonationIngest;
use forge_storage::{CredentialsRepo, DataProvider, StorageError, has_credentials_for};
use forge_types::IntegrationId;

use crate::integration_supervisor::{IntegrationFactory, RunningIntegration, TaskGroup};
use crate::integrations::{BuiltinObject, creds_of};

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

#[async_trait]
impl<P: DonationIntegration + 'static> IntegrationFactory for DonationFactory<P> {
    fn id(&self) -> IntegrationId {
        DonationProvider::provider(self.provider.as_ref()).clone()
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        has_credentials_for(self.creds.as_ref(), &self.id()).await
    }

    async fn start(&self) -> Result<RunningIntegration, String> {
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
            obs_client: None,
            vtube_client: None,
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
) -> Option<DonationFactory<forge_donatello::DonatelloProvider>> {
    let creds = creds_of(backend);
    let provider = forge_donatello::DonatelloProvider::new(
        forge_donatello::DonatelloConfig::new(endpoints),
        Arc::clone(&creds),
        forge_donatello::default_rate_limiter(),
    );
    match provider {
        Ok(provider) => Some(DonationFactory {
            provider: Arc::new(provider),
            creds,
            ingest: Arc::clone(ingest),
        }),
        Err(error) => {
            tracing::warn!(error = %error, "donatello integration unavailable");
            None
        }
    }
}

pub(crate) fn wire_monobank(
    backend: &Arc<dyn DataProvider>,
    endpoints: &PlatformEndpoints,
    ingest: &Arc<DonationIngest>,
) -> Option<DonationFactory<forge_monobank::MonobankProvider>> {
    let creds = creds_of(backend);
    let provider = forge_monobank::MonobankProvider::new(
        forge_monobank::MonobankConfig::new(endpoints),
        Arc::clone(&creds),
        forge_monobank::MonobankRateLimits::official(),
    );
    match provider {
        Ok(provider) => Some(DonationFactory {
            provider: Arc::new(provider),
            creds,
            ingest: Arc::clone(ingest),
        }),
        Err(error) => {
            tracing::warn!(error = %error, "monobank integration unavailable");
            None
        }
    }
}
