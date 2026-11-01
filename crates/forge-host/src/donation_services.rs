use std::sync::Arc;
use std::time::Duration;

use forge_donatello::DONATELLO_POLL_INTERVAL_SECS;
use forge_platform_core::{AdjustablePoll, BuiltinHealth, BuiltinStatus, DonationAccountSetup};
use forge_storage::{
    Setting, SettingsRepo, StorageError, Tally, read_stored_setting, write_setting,
};
use forge_types::IntegrationId;

#[derive(Clone)]
struct PollControl {
    setting: &'static Setting<Tally>,
    poll: Arc<dyn AdjustablePoll>,
}

#[derive(Clone)]
pub struct DonationService {
    status: Arc<dyn BuiltinStatus>,
    health: Arc<dyn BuiltinHealth>,
    setup: Arc<dyn DonationAccountSetup>,
    poll: Option<PollControl>,
}

impl DonationService {
    pub fn of<P>(provider: Arc<P>) -> Self
    where
        P: BuiltinStatus + BuiltinHealth + DonationAccountSetup + 'static,
    {
        Self {
            status: provider.clone(),
            health: provider.clone(),
            setup: provider,
            poll: None,
        }
    }

    #[must_use]
    pub fn with_adjustable_poll(
        mut self,
        setting: &'static Setting<Tally>,
        poll: Arc<dyn AdjustablePoll>,
    ) -> Self {
        self.poll = Some(PollControl { setting, poll });
        self
    }

    pub fn id(&self) -> &IntegrationId {
        self.status.id()
    }

    pub fn status(&self) -> Arc<dyn BuiltinStatus> {
        Arc::clone(&self.status)
    }

    pub fn health(&self) -> Arc<dyn BuiltinHealth> {
        Arc::clone(&self.health)
    }

    pub fn setup(&self) -> Arc<dyn DonationAccountSetup> {
        Arc::clone(&self.setup)
    }

    pub async fn shown_poll_interval(&self, settings: &dyn SettingsRepo) -> Option<Duration> {
        let control = self.poll.as_ref()?;
        Some(
            stored_poll_interval(control.setting, settings)
                .await
                .map_or(control.poll.default_interval(), |stored| {
                    control.poll.bounded_interval(stored)
                }),
        )
    }

    pub fn apply_poll_interval(&self, requested: Duration) -> Option<Duration> {
        let control = self.poll.as_ref()?;
        Some(control.poll.apply_interval(requested))
    }

    pub async fn keep_poll_interval(
        &self,
        settings: &dyn SettingsRepo,
        interval: Duration,
    ) -> Result<(), StorageError> {
        match &self.poll {
            Some(control) => write_setting(settings, control.setting, &interval.as_secs()).await,
            None => Ok(()),
        }
    }

    pub async fn restore_poll_interval(&self, settings: &dyn SettingsRepo) {
        if let Some(control) = &self.poll
            && let Some(interval) = stored_poll_interval(control.setting, settings).await
        {
            control.poll.apply_interval(interval);
        }
    }
}

#[derive(Clone, Default)]
pub struct DonationServices {
    services: Vec<DonationService>,
}

impl DonationServices {
    pub fn register(&mut self, service: DonationService) {
        self.services.push(service);
    }

    pub fn all(&self) -> &[DonationService] {
        &self.services
    }

    pub fn service(&self, id: &IntegrationId) -> Option<DonationService> {
        self.services
            .iter()
            .find(|service| service.id() == id)
            .cloned()
    }
}

pub fn donatello_service(provider: Arc<forge_donatello::DonatelloProvider>) -> DonationService {
    DonationService::of(Arc::clone(&provider))
        .with_adjustable_poll(&DONATELLO_POLL_INTERVAL_SECS, provider)
}

async fn stored_poll_interval(
    setting: &'static Setting<Tally>,
    settings: &dyn SettingsRepo,
) -> Option<Duration> {
    match read_stored_setting(settings, setting).await {
        Ok(stored) => stored.map(Duration::from_secs),
        Err(error) => {
            tracing::warn!(error = %error, "could not read the poll interval");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_donatello::{
        DEFAULT_POLL_INTERVAL, DonatelloConfig, DonatelloProvider, MAX_POLL_INTERVAL,
        MIN_POLL_INTERVAL,
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
        let provider = Arc::new(DonatelloProvider::new(
            config,
            Arc::clone(&backend) as Arc<dyn CredentialsRepo>,
            gateway,
        ));

        donatello_service(Arc::clone(&provider))
            .restore_poll_interval(backend.as_ref())
            .await;

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
