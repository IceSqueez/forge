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

pub async fn restore_donatello_poll_interval(
    provider: &DonatelloProvider,
    settings: &dyn SettingsRepo,
) {
    match settings.get_string(DONATELLO_POLL_INTERVAL_KEY).await {
        Ok(Some(stored)) => match stored.trim().parse::<u64>() {
            Ok(secs) => {
                provider.set_poll_interval(Duration::from_secs(secs));
            }
            Err(_) => {
                tracing::warn!(stored = %stored, "ignoring an unreadable Donatello poll interval");
            }
        },
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(error = %error, "could not read the Donatello poll interval");
        }
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
