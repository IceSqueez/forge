use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{Backoff, DedupSet, PlatformError};
use forge_storage::CredentialsRepo;
use forge_types::{Donation, IntegrationId};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::api::MonobankApi;
use crate::config::STATEMENT_CALL_INTERVAL;
use crate::credentials;
use crate::error::MonobankError;
use crate::jar::{JarDirectory, JarId};
use crate::normalize::donation_from_item;
use crate::status::{PollFailure, PollPhase, StatusBoard};
use crate::token::MonobankToken;
use crate::window::{Coverage, StatementWindow, WindowPlanner};
use crate::wire::{StatementItemWire, TRANSACTION_ID_FIELD, TRANSACTION_TIME_FIELD};

const SEEN_TRANSACTION_CAPACITY: usize = 4096;
const ERROR_BACKOFF_BASE: Duration = Duration::from_secs(5);
const ERROR_BACKOFF_CAP: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollControl {
    pub(crate) enabled: bool,
    pub(crate) generation: u64,
}

pub(crate) type DonationSink = mpsc::Sender<Result<Donation, PlatformError>>;

pub(crate) struct PollContext {
    pub(crate) provider: IntegrationId,
    pub(crate) api: Arc<MonobankApi>,
    pub(crate) jars: Arc<JarDirectory>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) planner: WindowPlanner,
    pub(crate) control: watch::Receiver<PollControl>,
    pub(crate) status: Arc<StatusBoard>,
    pub(crate) sink: DonationSink,
}

#[derive(Clone, PartialEq, Eq)]
struct Session {
    token: MonobankToken,
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

enum CycleError {
    Poll(MonobankError),
    SinkClosed,
}

impl From<MonobankError> for CycleError {
    fn from(error: MonobankError) -> Self {
        Self::Poll(error)
    }
}

enum Wake {
    Elapsed,
    ControlChanged,
    Closed,
}

#[derive(Default)]
struct Harvest {
    donations: Vec<Donation>,
    rejected: Vec<MonobankError>,
    oldest_item_unix: Option<i64>,
}

pub(crate) async fn run(mut ctx: PollContext) {
    let mut session: Option<Session> = None;
    let mut memory: Option<PollMemory> = None;
    let mut generation = None;
    let mut backoff = Backoff::new(ERROR_BACKOFF_BASE, ERROR_BACKOFF_CAP);
    loop {
        let control = *ctx.control.borrow_and_update();
        if !control.enabled {
            ctx.status.enter(PollPhase::Paused);
            match wait(&mut ctx, None).await {
                Wake::Closed => return,
                Wake::Elapsed | Wake::ControlChanged => continue,
            }
        }
        if generation != Some(control.generation) {
            generation = Some(control.generation);
            session = None;
            backoff.reset();
        }
        let delay = match cycle(&ctx, &mut session, &mut memory).await {
            Ok(()) => {
                backoff.reset();
                Some(STATEMENT_CALL_INTERVAL)
            }
            Err(CycleError::SinkClosed) => return,
            Err(CycleError::Poll(MonobankError::CoolingDown { retry_after_secs })) => {
                debug!(retry_after_secs, "monobank call budget cooling down");
                Some(Duration::from_secs(u64::from(retry_after_secs)))
            }
            Err(CycleError::Poll(error)) => {
                ctx.status.record_failure(&error);
                warn!(
                    failure = PollFailure::of(&error).label(),
                    error = %error,
                    "monobank poll failed"
                );
                let needs_user = error.requires_user_action();
                if needs_user {
                    session = None;
                }
                if ctx.sink.send(Err(error.into())).await.is_err() {
                    return;
                }
                (!needs_user).then(|| STATEMENT_CALL_INTERVAL + backoff.next_delay())
            }
        };
        if let Wake::Closed = wait(&mut ctx, delay).await {
            return;
        }
    }
}

async fn wait(ctx: &mut PollContext, delay: Option<Duration>) -> Wake {
    let sleep = async {
        match delay {
            Some(delay) => tokio::time::sleep(delay).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        () = sleep => Wake::Elapsed,
        changed = ctx.control.changed() => {
            if changed.is_ok() { Wake::ControlChanged } else { Wake::Closed }
        }
        () = ctx.sink.closed() => Wake::Closed,
    }
}

async fn cycle(
    ctx: &PollContext,
    session: &mut Option<Session>,
    memory: &mut Option<PollMemory>,
) -> Result<(), CycleError> {
    let current = match session {
        Some(current) => current.clone(),
        None => {
            ctx.status.enter(PollPhase::Starting);
            let credential = credentials::load(ctx.creds.as_ref())
                .await?
                .ok_or(MonobankError::MissingToken)?;
            let jar_id = credential.jar_id.ok_or(MonobankError::MissingJar)?;
            let jar = ctx.jars.find(&ctx.api, &credential.token, &jar_id).await?;
            ctx.status.record_jar(jar);
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
    let items = ctx
        .api
        .statement(
            &current.token,
            &current.jar,
            window.from_unix,
            window.to_unix,
        )
        .await?;
    let item_count = items.len();
    let harvest = harvest(&ctx.provider, &mut memory.seen, items, window);
    let oldest_item_unix = harvest.oldest_item_unix;
    let newest = emit(ctx, harvest).await?;
    memory.coverage.record(window, item_count, oldest_item_unix);
    ctx.status.record_poll(newest);
    Ok(())
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
                reason: format!("{:?} error in the statement item", error.classify()),
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
    mut harvest: Harvest,
) -> Result<Option<OffsetDateTime>, CycleError> {
    harvest
        .donations
        .sort_by_key(|donation| donation.occurred_at);
    let newest = harvest
        .donations
        .last()
        .map(|donation| donation.occurred_at);
    for donation in harvest.donations {
        ctx.sink
            .send(Ok(donation))
            .await
            .map_err(|_| CycleError::SinkClosed)?;
    }
    for error in harvest.rejected {
        ctx.status.record_rejected_donation(&error);
        warn!(error = %error, "monobank transaction skipped");
        ctx.sink
            .send(Err(error.into()))
            .await
            .map_err(|_| CycleError::SinkClosed)?;
    }
    Ok(newest)
}
