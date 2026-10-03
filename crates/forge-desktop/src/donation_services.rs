use std::sync::Arc;
use std::time::Duration;

use forge_donatello::DonatelloProvider;
use forge_monobank::MonobankProvider;
use forge_platform_core::{BuiltinHealth, BuiltinStatus, ConnectionState, IntegrationCategory};
use forge_storage::SettingsRepo;
use forge_types::IntegrationId;
use futures_util::StreamExt as _;
use gpui::{App, Entity};

use crate::integration_catalog::declarations;
use crate::platforms::PlatformConnectivity;

pub const DONATELLO_POLL_INTERVAL_KEY: &str = "donations.donatello.poll_interval_secs";

#[derive(Clone, Default)]
pub struct DonationServices {
    pub donatello: Option<Arc<DonatelloProvider>>,
    pub monobank: Option<Arc<MonobankProvider>>,
}

#[derive(Clone)]
pub enum DonationService {
    Donatello(Arc<DonatelloProvider>),
    Monobank(Arc<MonobankProvider>),
}

impl DonationService {
    pub fn id(&self) -> &IntegrationId {
        match self {
            Self::Donatello(provider) => BuiltinStatus::id(provider.as_ref()),
            Self::Monobank(provider) => BuiltinStatus::id(provider.as_ref()),
        }
    }

    fn status(&self) -> Arc<dyn BuiltinStatus> {
        match self {
            Self::Donatello(provider) => Arc::clone(provider) as Arc<dyn BuiltinStatus>,
            Self::Monobank(provider) => Arc::clone(provider) as Arc<dyn BuiltinStatus>,
        }
    }

    fn health(&self) -> Arc<dyn BuiltinHealth> {
        match self {
            Self::Donatello(provider) => Arc::clone(provider) as Arc<dyn BuiltinHealth>,
            Self::Monobank(provider) => Arc::clone(provider) as Arc<dyn BuiltinHealth>,
        }
    }
}

impl DonationServices {
    pub fn all(&self) -> Vec<DonationService> {
        let donatello = self.donatello.clone().map(DonationService::Donatello);
        let monobank = self.monobank.clone().map(DonationService::Monobank);
        donatello.into_iter().chain(monobank).collect()
    }

    pub fn service(&self, id: &IntegrationId) -> Option<DonationService> {
        self.all().into_iter().find(|service| service.id() == id)
    }
}

pub async fn stored_donatello_poll_interval(settings: &dyn SettingsRepo) -> Option<Duration> {
    match settings.get_string(DONATELLO_POLL_INTERVAL_KEY).await {
        Ok(Some(stored)) => match stored.trim().parse::<u64>() {
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => {
                tracing::warn!(stored = %stored, "ignoring an unreadable Donatello poll interval");
                None
            }
        },
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(error = %error, "could not read the Donatello poll interval");
            None
        }
    }
}

pub async fn restore_donatello_poll_interval(
    provider: &DonatelloProvider,
    settings: &dyn SettingsRepo,
) {
    if let Some(interval) = stored_donatello_poll_interval(settings).await {
        provider.set_poll_interval(interval);
    }
}

pub fn donation_provider_options() -> Vec<(String, String)> {
    declarations()
        .into_iter()
        .filter(|declaration| declaration.category == IntegrationCategory::DONATIONS)
        .map(|declaration| {
            (
                declaration.id.as_str().to_owned(),
                declaration.brand_name.to_owned(),
            )
        })
        .collect()
}

pub fn mirror_donation_connectivity(
    services: &DonationServices,
    connectivity: &Entity<PlatformConnectivity>,
    cx: &mut App,
) {
    for service in services.all() {
        let id = service.id().clone();
        let status = service.status();
        let mut deltas = service.health().stream();
        let connectivity = connectivity.clone();
        apply_connection(&connectivity, &id, status.as_ref(), cx);
        cx.spawn(async move |cx| {
            while deltas.next().await.is_some() {
                cx.update(|cx| apply_connection(&connectivity, &id, status.as_ref(), cx));
            }
        })
        .detach();
    }
}

fn apply_connection(
    connectivity: &Entity<PlatformConnectivity>,
    id: &IntegrationId,
    status: &dyn BuiltinStatus,
    cx: &mut App,
) {
    let connected = status.connection() == ConnectionState::Connected;
    connectivity.update(cx, |connectivity, cx| {
        if connectivity.set_service_connected(id, connected) {
            cx.notify();
        }
    });
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_donatello::{
        DEFAULT_POLL_INTERVAL, DonatelloConfig, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL,
    };
    use forge_platform_core::{DonationProvider, PlatformEndpoints};
    use forge_storage::CredentialsRepo;

    use super::*;
    use crate::test_support::test_backend;

    async fn interval_after_restoring(stored: Option<&str>) -> Duration {
        let (backend, _writes) = test_backend();
        if let Some(stored) = stored {
            backend
                .set_string(DONATELLO_POLL_INTERVAL_KEY, stored)
                .await
                .expect("the test backend stores");
        }
        let provider = DonatelloProvider::new(
            DonatelloConfig::new(&PlatformEndpoints::default()),
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            forge_donatello::default_rate_limiter(),
        )
        .expect("the provider builds offline");

        restore_donatello_poll_interval(&provider, backend.as_ref()).await;

        let mut feed = provider.donations();
        let first = feed.next().await;
        assert!(
            matches!(first, Some(Err(_))),
            "without a token the first poll reports the missing token"
        );
        provider.poll_status().poll_interval
    }

    #[tokio::test]
    async fn a_stored_interval_is_applied_and_held_inside_the_allowed_range() {
        for (stored, expected) in [
            ("60", Duration::from_secs(60)),
            (" 45 ", Duration::from_secs(45)),
            ("3", MIN_POLL_INTERVAL),
            ("100000", MAX_POLL_INTERVAL),
        ] {
            assert_eq!(
                interval_after_restoring(Some(stored)).await,
                expected,
                "{stored:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_unreadable_or_missing_interval_leaves_the_default_polling_pace() {
        for stored in [
            None,
            Some(""),
            Some("abc"),
            Some("-5"),
            Some("1.5"),
            Some("99999999999999999999"),
        ] {
            assert_eq!(
                interval_after_restoring(stored).await,
                DEFAULT_POLL_INTERVAL,
                "{stored:?}"
            );
        }
    }
}
