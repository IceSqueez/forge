use std::sync::Arc;
use std::time::Duration;

use forge_donatello::{DONATELLO_POLL_INTERVAL_SECS, DonatelloProvider};
use forge_monobank::MonobankProvider;
use forge_platform_core::{BuiltinHealth, BuiltinStatus};
use forge_storage::{SettingsRepo, read_stored_setting};
use forge_types::IntegrationId;

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

    pub fn status(&self) -> Arc<dyn BuiltinStatus> {
        match self {
            Self::Donatello(provider) => Arc::clone(provider) as Arc<dyn BuiltinStatus>,
            Self::Monobank(provider) => Arc::clone(provider) as Arc<dyn BuiltinStatus>,
        }
    }

    pub fn health(&self) -> Arc<dyn BuiltinHealth> {
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
    match read_stored_setting(settings, &DONATELLO_POLL_INTERVAL_SECS).await {
        Ok(stored) => stored.map(Duration::from_secs),
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

#[cfg(test)]
mod tests {
    use forge_donatello::{
        DEFAULT_POLL_INTERVAL, DonatelloConfig, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL,
    };
    use forge_platform_core::{DonationProvider, PlatformEndpoints};
    use forge_storage::CredentialsRepo;
    use futures_util::StreamExt as _;

    use super::*;
    use crate::test_support::test_backend;

    async fn interval_after_restoring(stored: Option<&str>) -> Duration {
        let (backend, _writes) = test_backend();
        if let Some(stored) = stored {
            backend.seed_raw(&DONATELLO_POLL_INTERVAL_SECS, stored);
        }
        let config = DonatelloConfig::new(&PlatformEndpoints::default());
        let gateway =
            forge_donatello::donatello_gateway(&config, forge_donatello::default_rate_limiter())
                .expect("the gateway builds offline");
        let provider = DonatelloProvider::new(
            config,
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            gateway,
        );

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
