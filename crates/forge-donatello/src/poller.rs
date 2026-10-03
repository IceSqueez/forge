use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{Backoff, DedupSet, PlatformError};
use forge_storage::CredentialsRepo;
use forge_types::{Donation, DonationOrigin, IntegrationId};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::api::DonatelloApi;
use crate::credentials;
use crate::error::DonatelloError;
use crate::normalize::donation_from_wire;
use crate::status::{PollFailure, PollPhase, StatusBoard};
use crate::token::DonatelloToken;
use crate::wire::{DONATION_ID_FIELD, DonateWire, DonatesPageWire};

const MAX_SCAN_PAGES: u64 = 500;
const ERROR_BACKOFF_BASE: Duration = Duration::from_secs(5);
const ERROR_BACKOFF_CAP: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollControl {
    pub(crate) enabled: bool,
    pub(crate) generation: u64,
    pub(crate) interval: Duration,
}

pub(crate) type DonationSink = mpsc::Sender<Result<Donation, PlatformError>>;

pub(crate) struct PollContext {
    pub(crate) provider: IntegrationId,
    pub(crate) api: Arc<DonatelloApi>,
    pub(crate) creds: Arc<dyn CredentialsRepo>,
    pub(crate) control: watch::Receiver<PollControl>,
    pub(crate) status: Arc<StatusBoard>,
    pub(crate) sink: DonationSink,
}

struct PollMemory {
    owner: DonatelloToken,
    seen: DedupSet,
    known_total: Option<u64>,
    baseline_taken: bool,
}

impl PollMemory {
    fn new(owner: DonatelloToken) -> Self {
        Self {
            owner,
            seen: DedupSet::unbounded(),
            known_total: None,
            baseline_taken: false,
        }
    }
}

enum CycleError {
    Poll(DonatelloError),
    SinkClosed,
}

impl From<DonatelloError> for CycleError {
    fn from(error: DonatelloError) -> Self {
        Self::Poll(error)
    }
}

enum Wake {
    Elapsed,
    ControlChanged,
    Closed,
}

struct Harvest {
    donations: Vec<Donation>,
    rejected: Vec<DonatelloError>,
    unseen: u64,
}

impl Harvest {
    fn empty() -> Self {
        Self {
            donations: Vec::new(),
            rejected: Vec::new(),
            unseen: 0,
        }
    }
}

pub(crate) async fn run(mut ctx: PollContext) {
    let mut session: Option<DonatelloToken> = None;
    let mut memory: Option<PollMemory> = None;
    let mut generation = None;
    let mut backoff = Backoff::new(ERROR_BACKOFF_BASE, ERROR_BACKOFF_CAP);
    loop {
        let control = *ctx.control.borrow_and_update();
        ctx.status.set_poll_interval(control.interval);
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
                Some(control.interval)
            }
            Err(CycleError::SinkClosed) => return,
            Err(CycleError::Poll(error)) => {
                ctx.status.record_failure(&error);
                warn!(
                    failure = PollFailure::of(&error).label(),
                    error = %error,
                    "donatello poll failed"
                );
                let needs_user = error.requires_user_action();
                if needs_user {
                    session = None;
                }
                if ctx.sink.send(Err(error.into())).await.is_err() {
                    return;
                }
                (!needs_user).then(|| control.interval + backoff.next_delay())
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
    session: &mut Option<DonatelloToken>,
    memory: &mut Option<PollMemory>,
) -> Result<(), CycleError> {
    let token = match session {
        Some(token) => token.clone(),
        None => {
            ctx.status.enter(PollPhase::Starting);
            let token = credentials::load(ctx.creds.as_ref())
                .await?
                .ok_or(DonatelloError::MissingToken)?;
            let account = ctx.api.account(&token).await?;
            ctx.status.record_account(account);
            *session = Some(token.clone());
            token
        }
    };
    let memory = match memory {
        Some(existing) if existing.owner == token => existing,
        slot => slot.insert(PollMemory::new(token.clone())),
    };
    let newest = if memory.baseline_taken {
        scan_for_new(ctx, &token, memory).await?
    } else {
        take_baseline(ctx, &token, memory).await?
    };
    ctx.status.record_poll(newest);
    Ok(())
}

async fn take_baseline(
    ctx: &PollContext,
    token: &DonatelloToken,
    memory: &mut PollMemory,
) -> Result<Option<OffsetDateTime>, CycleError> {
    let mut newest = None;
    let mut total = None;
    let mut page_index = 0;
    loop {
        let page = ctx.api.donates_page(token, page_index).await?;
        if page_index == 0 {
            total = page.total_count();
        }
        let finished = is_final_page(&page, page_index);
        let harvest = harvest(&ctx.provider, memory, page.content, DonationOrigin::History);
        newest = newest.max(emit(ctx, harvest).await?);
        if finished {
            break;
        }
        page_index += 1;
    }
    memory.known_total = total;
    memory.baseline_taken = true;
    Ok(newest)
}

async fn scan_for_new(
    ctx: &PollContext,
    token: &DonatelloToken,
    memory: &mut PollMemory,
) -> Result<Option<OffsetDateTime>, CycleError> {
    let first = ctx.api.donates_page(token, 0).await?;
    let total = first.total_count();
    let expected_new = total
        .zip(memory.known_total)
        .map_or(0, |(total, known)| total.saturating_sub(known));
    let page_count = first.page_count().unwrap_or(1).min(MAX_SCAN_PAGES);
    let mut found = harvest(&ctx.provider, memory, first.content, DonationOrigin::Live);
    let mut failure = None;
    for page_index in (1..page_count).rev() {
        if found.unseen >= expected_new {
            break;
        }
        match ctx.api.donates_page(token, page_index).await {
            Ok(page) => {
                let more = harvest(&ctx.provider, memory, page.content, DonationOrigin::Live);
                found.unseen += more.unseen;
                found.donations.extend(more.donations);
                found.rejected.extend(more.rejected);
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    let newest = emit(ctx, found).await?;
    if let Some(error) = failure {
        return Err(error.into());
    }
    memory.known_total = total.or(memory.known_total);
    Ok(newest)
}

fn is_final_page(page: &DonatesPageWire, page_index: u64) -> bool {
    let next = page_index + 1;
    page.last == Some(true)
        || page.content.is_empty()
        || page.page_count().is_some_and(|count| next >= count)
        || next >= MAX_SCAN_PAGES
}

fn harvest(
    provider: &IntegrationId,
    memory: &mut PollMemory,
    items: Vec<serde_json::Value>,
    origin: DonationOrigin,
) -> Harvest {
    let mut harvest = Harvest::empty();
    for item in items {
        let Some(donation_id) = item
            .get(DONATION_ID_FIELD)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        else {
            debug!("donatello list item without a donation id skipped");
            continue;
        };
        if !memory.seen.try_insert(donation_id.clone()) {
            continue;
        }
        harvest.unseen += 1;
        let parsed = serde_json::from_value::<DonateWire>(item)
            .map_err(|error| DonatelloError::InvalidDonation {
                donation_id,
                reason: format!("{:?} error in the donation record", error.classify()),
            })
            .and_then(|wire| donation_from_wire(provider, wire, origin));
        match parsed {
            Ok(donation) => harvest.donations.push(donation),
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
        warn!(error = %error, "donatello donation skipped");
        ctx.sink
            .send(Err(error.into()))
            .await
            .map_err(|_| CycleError::SinkClosed)?;
    }
    Ok(newest)
}
