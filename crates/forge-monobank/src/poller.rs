use std::sync::Arc;
use std::time::Duration;

use forge_events::{EventPublisher, EventSource, TypedEvent};
use forge_net::DedupSet;
use forge_platform_core::{PollScope, PollWorker};
use forge_storage::CredentialsRepo;
use forge_types::{Donation, IntegrationId, MoneyAmount, Secret};
use time::OffsetDateTime;
use tracing::debug;

use crate::api::MonobankApi;
use crate::credentials;
use crate::error::{MonobankError, redacted_serde_reason};
use crate::jar::{JarDirectory, JarId, MonobankJar};
use crate::jar_progress::{JarProgressTracker, newest_balance};
use crate::normalize::donation_from_item;
use crate::status::{PollFailure, PollerPhase, StatusBoard};
use crate::window::{Coverage, StatementWindow, WindowPlanner};
use crate::wire::{StatementItemWire, TRANSACTION_ID_FIELD, TRANSACTION_TIME_FIELD};

const SEEN_TRANSACTION_CAPACITY: usize = 4096;

pub(crate) struct PollContext {
    pub(crate) provider: IntegrationId,
    pub(crate) api: Arc<MonobankApi>,
    pub(crate) jars: Arc<JarDirectory>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) planner: WindowPlanner,
    pub(crate) status: Arc<StatusBoard>,
    pub(crate) progress_publisher: Option<Arc<dyn EventPublisher>>,
}

#[derive(Clone, PartialEq, Eq)]
struct Session {
    token: Secret,
    jar: JarId,
}

struct PollMemory {
    owner: Session,
    seen: DedupSet,
    coverage: Coverage,
}

impl PollMemory {
    fn new(owner: Session) -> Self {
        Self {
            owner,
            seen: DedupSet::bounded(SEEN_TRANSACTION_CAPACITY),
            coverage: Coverage::default(),
        }
    }
}

type CycleError = forge_platform_core::CycleError<MonobankError>;

pub(crate) struct Worker {
    ctx: PollContext,
    session: Option<Session>,
    memory: Option<PollMemory>,
    progress: JarProgressTracker,
}

impl Worker {
    pub(crate) fn new(ctx: PollContext) -> Self {
        Self {
            ctx,
            session: None,
            memory: None,
            progress: JarProgressTracker::default(),
        }
    }
}

impl PollWorker for Worker {
    type Subject = MonobankJar;
    type Failure = PollFailure;
    type Error = MonobankError;

    const LOG_NAME: &'static str = "monobank";

    fn forget_session(&mut self) {
        self.session = None;
    }

    async fn cycle(&mut self, scope: &PollScope) -> Result<(), CycleError> {
        cycle(
            &self.ctx,
            scope,
            &mut self.session,
            &mut self.memory,
            &mut self.progress,
        )
        .await
    }

    fn pacing_delay(&self, error: &MonobankError) -> Option<Duration> {
        let MonobankError::CoolingDown { retry_after_secs } = error else {
            return None;
        };
        debug!(retry_after_secs, "monobank call budget cooling down");
        Some(Duration::from_secs(u64::from(*retry_after_secs)))
    }
}

#[derive(Default)]
struct Harvest {
    donations: Vec<Donation>,
    rejected: Vec<MonobankError>,
    oldest_item_unix: Option<i64>,
}

async fn cycle(
    ctx: &PollContext,
    scope: &PollScope,
    session: &mut Option<Session>,
    memory: &mut Option<PollMemory>,
    progress: &mut JarProgressTracker,
) -> Result<(), CycleError> {
    let current = match session {
        Some(current) => current.clone(),
        None => {
            ctx.status.enter(PollerPhase::Starting);
            let credential = credentials::load(ctx.creds.as_ref())
                .await?
                .ok_or(MonobankError::MissingToken)?;
            let jar_id = credential.jar_id.ok_or(MonobankError::MissingJar)?;
            let jar = scope
                .interruptible(ctx.jars.find(&ctx.api, &credential.token, &jar_id))
                .await?;
            progress.connected(jar.clone());
            ctx.status.record_subject(jar);
            let started = Session {
                token: credential.token,
                jar: jar_id,
            };
            *session = Some(started.clone());
            started
        }
    };
    let memory = match memory {
        Some(existing) if existing.owner == current => existing,
        slot => slot.insert(PollMemory::new(current.clone())),
    };
    let window = ctx
        .planner
        .next(&memory.coverage, OffsetDateTime::now_utc().unix_timestamp());
    let items = scope
        .interruptible(ctx.api.statement(
            &current.token,
            &current.jar,
            window.from_unix,
            window.to_unix,
        ))
        .await?;
    let item_count = items.len();
    let current_balance = window
        .advances_coverage
        .then(|| newest_balance(&items))
        .flatten();
    let harvest = harvest(&ctx.provider, &mut memory.seen, items, window);
    let oldest_item_unix = harvest.oldest_item_unix;
    let newest = emit(ctx, scope, harvest).await?;
    memory.coverage.record(window, item_count, oldest_item_unix);
    ctx.status.record_poll(newest);
    publish_progress(ctx, progress, current_balance);
    Ok(())
}

fn publish_progress(
    ctx: &PollContext,
    progress: &mut JarProgressTracker,
    current_balance: Option<MoneyAmount>,
) {
    let Some(publisher) = &ctx.progress_publisher else {
        return;
    };
    if let Some(changed) = progress.observe(current_balance) {
        debug!(is_synced = changed.is_synced, "monobank jar progress");
        changed.publish(publisher.as_ref(), EventSource::Donation, None);
    }
}

fn harvest(
    provider: &IntegrationId,
    seen: &mut DedupSet,
    items: Vec<serde_json::Value>,
    window: StatementWindow,
) -> Harvest {
    let mut harvest = Harvest::default();
    for item in items {
        if let Some(time) = item
            .get(TRANSACTION_TIME_FIELD)
            .and_then(serde_json::Value::as_i64)
        {
            harvest.oldest_item_unix = Some(
                harvest
                    .oldest_item_unix
                    .map_or(time, |oldest| oldest.min(time)),
            );
        }
        let Some(transaction_id) = item
            .get(TRANSACTION_ID_FIELD)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        else {
            debug!("monobank statement item without a transaction id skipped");
            continue;
        };
        if !seen.try_insert(transaction_id.clone()) {
            continue;
        }
        let parsed = serde_json::from_value::<StatementItemWire>(item)
            .map_err(|error| MonobankError::InvalidDonation {
                donation_id: transaction_id,
                reason: format!("{} in the statement item", redacted_serde_reason(&error)),
            })
            .and_then(|wire| donation_from_item(provider, wire, window.origin));
        match parsed {
            Ok(Some(donation)) => harvest.donations.push(donation),
            Ok(None) => {}
            Err(error) => harvest.rejected.push(error),
        }
    }
    harvest
}

async fn emit(
    ctx: &PollContext,
    scope: &PollScope,
    harvest: Harvest,
) -> Result<Option<OffsetDateTime>, CycleError> {
    scope
        .emit(
            &ctx.status,
            harvest.donations,
            harvest.rejected,
            "monobank transaction skipped",
        )
        .await
}
