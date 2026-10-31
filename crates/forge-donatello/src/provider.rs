use std::sync::Arc;
use std::time::Duration;

use forge_events::EventPublisher;
use forge_net::{Backoff, Gateway};
use forge_platform_core::{DonationPoll, DonationProvider, DonationStream, PolledDonationProvider};
use forge_storage::CredentialsRepo;
use forge_types::IntegrationId;

use crate::api::{DonatelloAccount, DonatelloApi};
use crate::config::{DonatelloConfig, DonatelloTiming};
use crate::credentials;
use crate::error::DonatelloError;
use crate::health;
use crate::integration::DONATELLO_INTEGRATION;
use crate::poller::{PollContext, Worker};
use crate::status::{PollFailure, PollStatus};
use crate::token::parse_token;

pub struct DonatelloProvider {
    pub(crate) poll: DonationPoll<DonatelloAccount, PollFailure>,
    config: DonatelloConfig,
    api: Arc<DonatelloApi>,
    creds: Arc<dyn CredentialsRepo>,
    timing: DonatelloTiming,
}

impl DonatelloProvider {
    pub fn new(config: DonatelloConfig, creds: Arc<dyn CredentialsRepo>, gateway: Gateway) -> Self {
        let timing = DonatelloTiming::default();
        let interval = timing.bounded_poll_interval(config.poll_interval);
        Self {
            poll: DonationPoll::new(DONATELLO_INTEGRATION.id, interval, health::metrics),
            config,
            api: Arc::new(DonatelloApi::new(gateway)),
            creds,
            timing,
        }
    }

    pub fn with_timing(mut self, timing: DonatelloTiming) -> Self {
        self.timing = timing;
        let interval = self.set_poll_interval(self.config.poll_interval);
        self.poll.board().set_poll_interval(interval);
        self
    }

    pub fn with_event_publisher(self, publisher: Arc<dyn EventPublisher>) -> Self {
        self.poll.attach_publisher(publisher);
        self
    }

    pub fn poll_status(&self) -> PollStatus {
        self.poll.status()
    }

    pub fn is_polling_enabled(&self) -> bool {
        self.poll.is_enabled()
    }

    pub fn bounded_poll_interval(&self, requested: Duration) -> Duration {
        self.timing.bounded_poll_interval(requested)
    }

    pub fn set_poll_interval(&self, requested: Duration) -> Duration {
        let interval = self.bounded_poll_interval(requested);
        self.poll.set_interval(interval);
        interval
    }

    pub async fn verify_token(&self, raw: &str) -> Result<DonatelloAccount, DonatelloError> {
        let token = parse_token(raw)?;
        self.api.account(&token).await
    }

    pub async fn save_token(&self, raw: &str) -> Result<DonatelloAccount, DonatelloError> {
        let token = parse_token(raw)?;
        let account = self.api.account(&token).await?;
        credentials::store(self.creds.as_ref(), &token).await?;
        self.poll.restart();
        Ok(account)
    }

    pub async fn remove_token(&self) -> Result<bool, DonatelloError> {
        let removed = credentials::delete(self.creds.as_ref()).await?;
        self.poll.forget_session();
        Ok(removed)
    }
}

impl PolledDonationProvider for DonatelloProvider {
    type Subject = DonatelloAccount;
    type Failure = PollFailure;

    fn poll(&self) -> &DonationPoll<DonatelloAccount, PollFailure> {
        &self.poll
    }

    fn brand_name(&self) -> &'static str {
        DONATELLO_INTEGRATION.brand_name
    }

    fn api_base_url(&self) -> &str {
        &self.config.base_url
    }
}

impl DonationProvider for DonatelloProvider {
    fn provider(&self) -> &IntegrationId {
        self.poll.id()
    }

    fn donations(&self) -> DonationStream {
        let worker = Worker::new(PollContext {
            provider: self.poll.id().clone(),
            api: Arc::clone(&self.api),
            creds: Arc::clone(&self.creds),
            status: Arc::clone(self.poll.board()),
        });
        let backoff = Backoff::new(
            self.timing.error_backoff_base,
            self.timing.error_backoff_cap,
        );
        self.poll
            .start(worker, backoff, DonatelloError::NoRuntime.into())
    }
}
