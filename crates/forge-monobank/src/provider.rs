use std::sync::Arc;

use forge_platform_core::{DonationProvider, DonationStream};
use forge_storage::CredentialsRepo;
use forge_types::IntegrationId;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;

use crate::api::{MonobankApi, MonobankRateLimits};
use crate::config::MonobankConfig;
use crate::credentials::{self, MonobankCredential};
use crate::error::MonobankError;
use crate::integration::MONOBANK_INTEGRATION;
use crate::jar::{JarDirectory, JarId, MonobankJar};
use crate::poller::{self, PollContext, PollControl};
use crate::status::{PollStatus, StatusBoard};
use crate::token::MonobankToken;
use crate::window::WindowPlanner;

const DONATION_BUFFER: usize = 64;

pub struct MonobankProvider {
    pub(crate) id: IntegrationId,
    pub(crate) config: MonobankConfig,
    api: Arc<MonobankApi>,
    jars: Arc<JarDirectory>,
    creds: Arc<dyn CredentialsRepo>,
    control: watch::Sender<PollControl>,
    pub(crate) status: Arc<StatusBoard>,
}

impl MonobankProvider {
    pub fn new(
        config: MonobankConfig,
        creds: Arc<dyn CredentialsRepo>,
        limits: MonobankRateLimits,
    ) -> Result<Self, MonobankError> {
        let api = Arc::new(MonobankApi::new(&config, limits)?);
        let (control, _) = watch::channel(PollControl {
            enabled: true,
            generation: 0,
        });
        Ok(Self {
            id: MONOBANK_INTEGRATION.id,
            config,
            api,
            jars: Arc::new(JarDirectory::default()),
            creds,
            control,
            status: Arc::new(StatusBoard::new()),
        })
    }

    pub fn poll_status(&self) -> PollStatus {
        self.status.snapshot()
    }

    pub fn is_polling_enabled(&self) -> bool {
        self.control.borrow().enabled
    }

    pub async fn verify_token(&self, raw: &str) -> Result<Vec<MonobankJar>, MonobankError> {
        let token = MonobankToken::parse(raw)?;
        self.jars.jars(&self.api, &token).await
    }

    pub async fn stored_jars(&self) -> Result<Vec<MonobankJar>, MonobankError> {
        let credential = self.stored_credential().await?;
        self.jars.jars(&self.api, &credential.token).await
    }

    pub async fn selected_jar_id(&self) -> Result<Option<String>, MonobankError> {
        Ok(credentials::load(self.creds.as_ref())
            .await?
            .and_then(|credential| credential.jar_id)
            .map(|jar| jar.as_str().to_owned()))
    }

    pub async fn save_token(&self, raw: &str, jar_id: &str) -> Result<MonobankJar, MonobankError> {
        let token = MonobankToken::parse(raw)?;
        self.store_selection(token, jar_id).await
    }

    pub async fn select_jar(&self, jar_id: &str) -> Result<MonobankJar, MonobankError> {
        let credential = self.stored_credential().await?;
        self.store_selection(credential.token, jar_id).await
    }

    pub async fn remove_token(&self) -> Result<bool, MonobankError> {
        let removed = credentials::delete(self.creds.as_ref()).await?;
        self.jars.forget();
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

    async fn stored_credential(&self) -> Result<MonobankCredential, MonobankError> {
        credentials::load(self.creds.as_ref())
            .await?
            .ok_or(MonobankError::MissingToken)
    }

    async fn store_selection(
        &self,
        token: MonobankToken,
        jar_id: &str,
    ) -> Result<MonobankJar, MonobankError> {
        let jar_id = JarId::parse(jar_id)?;
        let jar = self.jars.find(&self.api, &token, &jar_id).await?;
        credentials::store(
            self.creds.as_ref(),
            &MonobankCredential {
                token,
                jar_id: Some(jar_id),
            },
        )
        .await?;
        self.restart();
        Ok(jar)
    }
}

impl DonationProvider for MonobankProvider {
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
                    jars: Arc::clone(&self.jars),
                    creds: Arc::clone(&self.creds),
                    planner: WindowPlanner {
                        history_lookback: self.config.history_lookback,
                        overlap: self.config.window_overlap,
                    },
                    control: self.control.subscribe(),
                    status: Arc::clone(&self.status),
                    sink,
                }));
            }
            Err(_) => {
                let _ = sink.try_send(Err(MonobankError::NoRuntime.into()));
            }
        }
        Box::pin(ReceiverStream::new(receiver))
    }
}
