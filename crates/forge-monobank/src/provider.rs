use std::sync::Arc;

use forge_events::EventPublisher;
use forge_net::Backoff;
use forge_platform_core::{DonationPoll, DonationProvider, DonationStream, PolledDonationProvider};
use forge_storage::CredentialsRepo;
use forge_types::IntegrationId;
use forge_types::Secret;

use crate::api::{MonobankApi, MonobankGateways};
use crate::config::{MonobankConfig, MonobankTiming};
use crate::credentials::{self, MonobankCredential};
use crate::error::MonobankError;
use crate::health;
use crate::integration::MONOBANK_INTEGRATION;
use crate::jar::{JarDirectory, JarId, MonobankJar};
use crate::poller::{PollContext, Worker};
use crate::status::{PollFailure, PollStatus};
use crate::token::parse_token;
use crate::window::WindowPlanner;

pub struct MonobankProvider {
    pub(crate) poll: DonationPoll<MonobankJar, PollFailure>,
    config: MonobankConfig,
    api: Arc<MonobankApi>,
    jars: Arc<JarDirectory>,
    creds: Arc<dyn CredentialsRepo>,
    timing: MonobankTiming,
}

impl MonobankProvider {
    pub fn new(
        config: MonobankConfig,
        creds: Arc<dyn CredentialsRepo>,
        gateways: MonobankGateways,
    ) -> Self {
        let timing = MonobankTiming::default();
        Self {
            poll: DonationPoll::new(
                MONOBANK_INTEGRATION.id,
                timing.statement_call_interval,
                health::metrics,
            ),
            config,
            api: Arc::new(MonobankApi::new(gateways)),
            jars: Arc::new(JarDirectory::default()),
            creds,
            timing,
        }
    }

    pub fn with_timing(mut self, timing: MonobankTiming) -> Self {
        self.timing = timing;
        self.poll.set_interval(timing.statement_call_interval);
        self.poll
            .board()
            .set_poll_interval(timing.statement_call_interval);
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

    pub async fn verify_token(&self, raw: &str) -> Result<Vec<MonobankJar>, MonobankError> {
        let token = parse_token(raw)?;
        self.jars.jars(&self.api, &token).await
    }

    pub async fn stored_jars(&self) -> Result<Vec<MonobankJar>, MonobankError> {
        let credential = self.stored_credential().await?;
        self.jars.jars(&self.api, &credential.token).await
    }

    pub async fn selected_jar_id(&self) -> Result<Option<JarId>, MonobankError> {
        Ok(credentials::load(self.creds.as_ref())
            .await?
            .and_then(|credential| credential.jar_id))
    }

    pub async fn save_token(&self, raw: &str, jar_id: JarId) -> Result<MonobankJar, MonobankError> {
        let token = parse_token(raw)?;
        self.store_selection(token, jar_id).await
    }

    pub async fn select_jar(&self, jar_id: JarId) -> Result<MonobankJar, MonobankError> {
        let credential = self.stored_credential().await?;
        self.store_selection(credential.token, jar_id).await
    }

    pub async fn remove_token(&self) -> Result<bool, MonobankError> {
        let removed = credentials::delete(self.creds.as_ref()).await?;
        self.jars.forget();
        self.poll.forget_session();
        Ok(removed)
    }

    async fn stored_credential(&self) -> Result<MonobankCredential, MonobankError> {
        credentials::load(self.creds.as_ref())
            .await?
            .ok_or(MonobankError::MissingToken)
    }

    async fn store_selection(
        &self,
        token: Secret,
        jar_id: JarId,
    ) -> Result<MonobankJar, MonobankError> {
        let jar = self.jars.find(&self.api, &token, &jar_id).await?;
        credentials::store(
            self.creds.as_ref(),
            &MonobankCredential {
                token,
                jar_id: Some(jar_id),
            },
        )
        .await?;
        self.poll.restart();
        Ok(jar)
    }
}

impl PolledDonationProvider for MonobankProvider {
    type Subject = MonobankJar;
    type Failure = PollFailure;

    fn poll(&self) -> &DonationPoll<MonobankJar, PollFailure> {
        &self.poll
    }

    fn brand_name(&self) -> &'static str {
        MONOBANK_INTEGRATION.brand_name
    }

    fn api_base_url(&self) -> &str {
        &self.config.base_url
    }
}

impl DonationProvider for MonobankProvider {
    fn provider(&self) -> &IntegrationId {
        self.poll.id()
    }

    fn donations(&self) -> DonationStream {
        let worker = Worker::new(PollContext {
            provider: self.poll.id().clone(),
            api: Arc::clone(&self.api),
            jars: Arc::clone(&self.jars),
            creds: Arc::clone(&self.creds),
            planner: WindowPlanner {
                history_lookback: self.config.history_lookback,
                overlap: self.config.window_overlap,
            },
            status: Arc::clone(self.poll.board()),
        });
        let backoff = Backoff::new(
            self.timing.error_backoff_base,
            self.timing.error_backoff_cap,
        );
        self.poll
            .start(worker, backoff, MonobankError::NoRuntime.into())
    }
}
