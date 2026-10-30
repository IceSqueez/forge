use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;

use forge_net::{Backoff, DedupSet};
use forge_platform_core::PlatformError;
use forge_storage::CredentialsRepo;
use forge_types::{Donation, DonationOrigin, IntegrationId, Secret};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::api::{DonatelloApi, PAGE_SIZE};
use crate::config::DonatelloTiming;
use crate::credentials;
use crate::error::DonatelloError;
use crate::normalize::{created_at_instant, donation_from_wire};
use crate::status::{PollFailure, PollPhase, StatusBoard};
use crate::wire::{CREATED_AT_FIELD, DONATION_ID_FIELD, DonateWire, DonatesPageWire};

const MAX_SCAN_PAGES: u64 = 500;

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
    pub(crate) timing: DonatelloTiming,
    pub(crate) active_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListOrder {
    NewestFirst,
    OldestFirst,
}

struct PollMemory {
    owner: Secret,
    seen: DedupSet,
    known_total: Option<u64>,
    baseline_taken: bool,
    order: Option<ListOrder>,
    pages_walked_before_failure: u64,
}

impl PollMemory {
    fn new(owner: Secret) -> Self {
        Self {
            owner,
            seen: DedupSet::unbounded(),
            known_total: None,
            baseline_taken: false,
            order: None,
            pages_walked_before_failure: 0,
        }
    }
}

enum CycleError {
    Poll(DonatelloError),
    SinkClosed,
    Interrupted,
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
    let _running = ctx.status.poller_started();
    let mut session: Option<Secret> = None;
    let mut memory: Option<PollMemory> = None;
    let mut generation = None;
    let mut backoff = Backoff::new(ctx.timing.error_backoff_base, ctx.timing.error_backoff_cap);
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
            ctx.active_generation = control.generation;
            session = None;
            backoff.reset();
        }
        let delay = match cycle(&ctx, &mut session, &mut memory).await {
            Ok(()) => {
                backoff.reset();
                Some(control.interval)
            }
            Err(CycleError::SinkClosed) => return,
            Err(CycleError::Interrupted) => continue,
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

fn cycle_is_stale(control: &PollControl, generation: u64) -> bool {
    !control.enabled || control.generation != generation
}

async fn interruptible<T, E: Into<CycleError>>(
    ctx: &PollContext,
    work: impl Future<Output = Result<T, E>>,
) -> Result<T, CycleError> {
    let generation = ctx.active_generation;
    let mut control = ctx.control.clone();
    tokio::select! {
        outcome = work => outcome.map_err(Into::into),
        _ = control.wait_for(|control| cycle_is_stale(control, generation)) => {
            Err(CycleError::Interrupted)
        }
        () = ctx.sink.closed() => Err(CycleError::SinkClosed),
    }
}

async fn cycle(
    ctx: &PollContext,
    session: &mut Option<Secret>,
    memory: &mut Option<PollMemory>,
) -> Result<(), CycleError> {
    let token = match session {
        Some(token) => token.clone(),
        None => {
            ctx.status.enter(PollPhase::Starting);
            let token = credentials::load(ctx.creds.as_ref())
                .await?
                .ok_or(DonatelloError::MissingToken)?;
            let account = interruptible(ctx, ctx.api.account(&token)).await?;
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
    token: &Secret,
    memory: &mut PollMemory,
) -> Result<Option<OffsetDateTime>, CycleError> {
    let mut newest = None;
    let mut total = None;
    let mut page_index = 0;
    loop {
        let mut page = interruptible(ctx, ctx.api.donates_page(token, page_index)).await?;
        if page_index == 0 {
            total = page.total_count();
            memory.order = list_order(&page.content);
        }
        let finished = is_final_page(&page, page_index);
        if memory.order == Some(ListOrder::OldestFirst)
            && let Some(total) = total
        {
            page.content
                .truncate(items_before_arrivals(total, page_index));
        }
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
    token: &Secret,
    memory: &mut PollMemory,
) -> Result<Option<OffsetDateTime>, CycleError> {
    let first = interruptible(ctx, ctx.api.donates_page(token, 0)).await?;
    let total = first.total_count();
    memory.order = list_order(&first.content).or(memory.order);
    let expected_new = total.zip(memory.known_total).map_or(0, |(total, known)| {
        arrivals_to_look_for(total, known, memory.order)
    });
    let page_count = first.page_count().unwrap_or(1).min(MAX_SCAN_PAGES);
    let first_listed = first.content.len();
    let mut found = harvest(&ctx.provider, memory, first.content, DonationOrigin::Live);
    let mut page_was_all_arrivals =
        memory.order == Some(ListOrder::NewestFirst) && all_arrivals(found.unseen, first_listed);
    let mut failure = None;
    let mut pages_walked = 0;
    let mut pages_without_arrivals = 0;
    for page_index in walk_after_first_page(memory.order, page_count) {
        if found.unseen >= expected_new && !page_was_all_arrivals {
            break;
        }
        match interruptible(ctx, ctx.api.donates_page(token, page_index)).await {
            Ok(page) => {
                let listed = page.content.len();
                let more = harvest(&ctx.provider, memory, page.content, DonationOrigin::Live);
                pages_walked += 1;
                page_was_all_arrivals = memory.order.is_some() && all_arrivals(more.unseen, listed);
                if more.unseen == 0 {
                    pages_without_arrivals += 1;
                }
                let reached_known_history = memory.order.is_some()
                    && pages_without_arrivals > memory.pages_walked_before_failure;
                found.unseen += more.unseen;
                found.donations.extend(more.donations);
                found.rejected.extend(more.rejected);
                if reached_known_history {
                    break;
                }
            }
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    let unseen_found = found.unseen;
    let newest = emit(ctx, found).await?;
    if let Some(error) = failure {
        memory.pages_walked_before_failure += pages_walked;
        memory.known_total = memory
            .known_total
            .map(|known| known.saturating_add(unseen_found));
        return Err(error);
    }
    memory.pages_walked_before_failure = 0;
    memory.known_total = total.or(memory.known_total);
    Ok(newest)
}

fn all_arrivals(unseen: u64, listed: usize) -> bool {
    unseen > 0 && u64::try_from(listed).is_ok_and(|listed| unseen == listed)
}

fn expected_arrivals(total: u64, known: u64) -> u64 {
    if total < known { 1 } else { total - known }
}

fn arrivals_to_look_for(total: u64, known: u64, order: Option<ListOrder>) -> u64 {
    let expected = expected_arrivals(total, known);
    match order {
        Some(ListOrder::OldestFirst) => expected.max(1),
        Some(ListOrder::NewestFirst) | None => expected,
    }
}

fn walk_after_first_page(order: Option<ListOrder>, page_count: u64) -> Vec<u64> {
    let forward = 1..page_count;
    match order {
        Some(ListOrder::NewestFirst) => forward.collect(),
        Some(ListOrder::OldestFirst) | None => forward.rev().collect(),
    }
}

fn items_before_arrivals(total_at_start: u64, page_index: u64) -> usize {
    let already_listed = page_index.saturating_mul(PAGE_SIZE);
    usize::try_from(total_at_start.saturating_sub(already_listed)).unwrap_or(usize::MAX)
}

fn list_order(items: &[serde_json::Value]) -> Option<ListOrder> {
    let mut created = items.iter().filter_map(|item| {
        let text = item.get(CREATED_AT_FIELD)?.as_str()?;
        created_at_instant(text).ok()
    });
    let first = created.next()?;
    let last = created.next_back()?;
    match first.cmp(&last) {
        Ordering::Greater => Some(ListOrder::NewestFirst),
        Ordering::Less => Some(ListOrder::OldestFirst),
        Ordering::Equal => None,
    }
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        ListOrder, MAX_SCAN_PAGES, expected_arrivals, is_final_page, items_before_arrivals,
        list_order, walk_after_first_page,
    };
    use crate::wire::DonatesPageWire;

    fn page(body: Value) -> DonatesPageWire {
        serde_json::from_value(body).unwrap()
    }

    #[test]
    fn final_page_is_detected_from_any_end_marker() {
        let item = json!({ "pubId": "D-1" });
        for (body, index, last) in [
            (json!({ "content": [item], "last": true }), 0, true),
            (json!({ "content": [] }), 0, true),
            (json!({ "content": [item], "pages": 3 }), 2, true),
            (json!({ "content": [item], "pages": 3 }), 1, false),
            (
                json!({ "content": [item], "pages": 3, "last": false }),
                0,
                false,
            ),
            (json!({ "content": [item] }), 7, false),
        ] {
            assert_eq!(
                is_final_page(&page(body.clone()), index),
                last,
                "{body} at {index}"
            );
        }
    }

    fn created(stamps: &[&str]) -> Vec<Value> {
        stamps.iter().map(|at| json!({ "createdAt": at })).collect()
    }

    #[test]
    fn list_order_compares_the_first_and_last_readable_stamps() {
        let early = "2026-07-01 10:00:00";
        let late = "2026-07-01 11:00:00";
        for (items, order) in [
            (created(&[late, early]), Some(ListOrder::NewestFirst)),
            (created(&[early, late]), Some(ListOrder::OldestFirst)),
            (created(&[early, early]), None),
            (created(&[early]), None),
            (Vec::new(), None),
            (
                created(&["not a date", late, early]),
                Some(ListOrder::NewestFirst),
            ),
            (
                vec![json!({ "createdAt": 7 }), json!({ "createdAt": early })],
                None,
            ),
        ] {
            assert_eq!(list_order(&items), order, "{items:?}");
        }
    }

    #[test]
    fn a_shrunken_total_still_expects_one_arrival_and_a_grown_one_expects_the_difference() {
        for (total, known, expected) in [(44, 45, 1), (45, 45, 0), (46, 45, 1), (70, 45, 25)] {
            assert_eq!(expected_arrivals(total, known), expected, "{total}/{known}");
        }
    }

    #[test]
    fn the_walk_runs_forward_only_on_a_list_known_to_be_newest_first() {
        for (order, walk) in [
            (Some(ListOrder::NewestFirst), vec![1, 2, 3]),
            (Some(ListOrder::OldestFirst), vec![3, 2, 1]),
            (None, vec![3, 2, 1]),
        ] {
            assert_eq!(walk_after_first_page(order, 4), walk, "{order:?}");
        }
        assert!(walk_after_first_page(None, 1).is_empty());
    }

    #[test]
    fn a_page_keeps_only_the_items_listed_before_the_baseline_started() {
        for (total, page, kept) in [
            (45, 2, 5),
            (40, 1, 20),
            (40, 2, 0),
            (41, 2, 1),
            (45, 3, 0),
            (0, 0, 0),
        ] {
            assert_eq!(items_before_arrivals(total, page), kept, "{total}/{page}");
        }
    }

    #[test]
    fn page_walk_stops_at_the_scan_cap_even_without_end_markers() {
        let endless = page(json!({ "content": [{ "pubId": "D-1" }] }));
        assert!(!is_final_page(&endless, MAX_SCAN_PAGES - 2));
        assert!(is_final_page(&endless, MAX_SCAN_PAGES - 1));
    }
}
