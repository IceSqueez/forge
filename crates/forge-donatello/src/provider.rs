use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{DonationProvider, DonationStream, RateLimiter};
use forge_storage::CredentialsRepo;
use forge_types::IntegrationId;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::{DonatelloAccount, DonatelloApi};
use crate::config::{DonatelloConfig, bounded_poll_interval};
use crate::credentials;
use crate::error::DonatelloError;
use crate::integration::DONATELLO_INTEGRATION;
use crate::poller::{self, PollContext, PollControl};
use crate::status::{PollStatus, StatusBoard};
use crate::token::DonatelloToken;

const DONATION_BUFFER: usize = 64;

pub struct DonatelloProvider {
    pub(crate) id: IntegrationId,
    pub(crate) config: DonatelloConfig,
    api: Arc<DonatelloApi>,
    creds: Arc<dyn CredentialsRepo>,
    control: watch::Sender<PollControl>,
    pub(crate) status: Arc<StatusBoard>,
}

impl DonatelloProvider {
    pub fn new(
        config: DonatelloConfig,
        creds: Arc<dyn CredentialsRepo>,
        limiter: Arc<dyn RateLimiter>,
    ) -> Result<Self, DonatelloError> {
        let interval = bounded_poll_interval(config.poll_interval);
        let api = Arc::new(DonatelloApi::new(&config, limiter)?);
        let (control, _) = watch::channel(PollControl {
            enabled: true,
            generation: 0,
            interval,
        });
        Ok(Self {
            id: DONATELLO_INTEGRATION.id,
            config,
            api,
            creds,
            control,
            status: Arc::new(StatusBoard::new(interval)),
        })
    }

    pub fn poll_status(&self) -> PollStatus {
        self.status.snapshot()
    }

    pub fn is_polling_enabled(&self) -> bool {
        self.control.borrow().enabled
    }

    pub fn set_poll_interval(&self, requested: Duration) -> Duration {
        let interval = bounded_poll_interval(requested);
        self.control.send_if_modified(|control| {
            let changed = control.interval != interval;
            control.interval = interval;
            changed
        });
        interval
    }

    pub async fn verify_token(&self, raw: &str) -> Result<DonatelloAccount, DonatelloError> {
        let token = DonatelloToken::parse(raw)?;
        self.api.account(&token).await
    }

    pub async fn save_token(&self, raw: &str) -> Result<DonatelloAccount, DonatelloError> {
        let token = DonatelloToken::parse(raw)?;
        let account = self.api.account(&token).await?;
        credentials::store(self.creds.as_ref(), &token).await?;
        self.restart();
        Ok(account)
    }

    pub async fn remove_token(&self) -> Result<bool, DonatelloError> {
        let removed = credentials::delete(self.creds.as_ref()).await?;
        self.control.send_modify(|control| {
            control.generation = control.generation.wrapping_add(1);
        });
        Ok(removed)
    }

    pub(crate) fn restart(&self) {
        self.control.send_modify(|control| {
            control.enabled = true;
            control.generation = control.generation.wrapping_add(1);
        });
    }

    pub(crate) fn pause(&self) {
        self.control.send_if_modified(|control| {
            let changed = control.enabled;
            control.enabled = false;
            changed
        });
    }
}

impl DonationProvider for DonatelloProvider {
    fn provider(&self) -> &IntegrationId {
        &self.id
    }

    fn donations(&self) -> DonationStream {
        let (sink, receiver) = mpsc::channel(DONATION_BUFFER);
        match Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(poller::run(PollContext {
                    provider: self.id.clone(),
                    api: Arc::clone(&self.api),
                    creds: Arc::clone(&self.creds),
                    control: self.control.subscribe(),
                    status: Arc::clone(&self.status),
                    sink,
                }));
            }
            Err(_) => {
                let _ = sink.try_send(Err(DonatelloError::NoRuntime.into()));
            }
        }
        Box::pin(ReceiverStream::new(receiver))
    }
}
