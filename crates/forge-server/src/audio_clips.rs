use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use forge_storage::OverlayId;
use forge_types::Redacted;
use rand::Rng as _;
use tokio::sync::{Notify, oneshot};

use crate::ServerError;
use crate::routes::audio::{clip_path, report_path};

const CAPABILITY_BYTE_LEN: usize = 32;
const WAVE_MEDIA_TYPE: &str = "audio/wav";

pub const MAX_CLIP_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_TOTAL_CLIP_BYTES: usize = 4 * MAX_CLIP_BYTES;

const FETCH_WINDOW: Duration = Duration::from_secs(30);
const VERDICT_SLACK: Duration = Duration::from_secs(15);
const MAX_VERDICT_WINDOW: Duration = Duration::from_secs(300);
const MAX_CLIP_LIFETIME: Duration = FETCH_WINDOW.saturating_add(MAX_VERDICT_WINDOW);
const MIN_FETCH_BUDGET: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipMediaType {
    Wave,
}

impl ClipMediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wave => WAVE_MEDIA_TYPE,
        }
    }
}

impl fmt::Display for ClipMediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ClipCapability(String);

impl ClipCapability {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClipCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClipCapability({Redacted:?})")
    }
}

pub struct ClipOffer {
    pub bytes: Vec<u8>,
    pub media_type: ClipMediaType,
    pub duration_ms: u64,
}

pub struct ClipTicket {
    capability: ClipCapability,
    clip_path: String,
    report_path: String,
}

impl ClipTicket {
    pub fn capability(&self) -> &ClipCapability {
        &self.capability
    }

    pub fn clip_path(&self) -> &str {
        &self.clip_path
    }

    pub fn report_path(&self) -> &str {
        &self.report_path
    }
}

impl fmt::Debug for ClipTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ClipTicket({Redacted:?})")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipOutcome {
    Played,
    Refused { reason: String },
    NeverFetched,
    NoVerdict,
    Revoked,
    ServerStopped,
}

pub struct ClipOutcomeHandle(oneshot::Receiver<ClipOutcome>);

impl ClipOutcomeHandle {
    pub async fn recv(self) -> ClipOutcome {
        self.0.await.unwrap_or(ClipOutcome::ServerStopped)
    }
}

#[derive(Clone)]
pub(crate) struct ClipPayload {
    pub(crate) bytes: Bytes,
    pub(crate) media_type: ClipMediaType,
}

struct ClipEntry {
    owner: OverlayId,
    duration_ms: u64,
    deadline: Instant,
    expires_by: Instant,
    held_remaining: Option<Duration>,
    payload: Option<ClipPayload>,
    fetch_budget: usize,
    fetches: usize,
    refusals: usize,
    first_refusal: Option<String>,
    outcome: oneshot::Sender<ClipOutcome>,
}

#[derive(Default)]
struct StoreInner {
    entries: HashMap<String, ClipEntry>,
    pending_bytes: usize,
    sweeping: bool,
}

pub struct AudioClipStore {
    inner: Mutex<StoreInner>,
    deadline_moved: Notify,
}

impl AudioClipStore {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(StoreInner::default()),
            deadline_moved: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, StoreInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn offer(
        self: &Arc<Self>,
        owner: &OverlayId,
        offer: ClipOffer,
    ) -> Result<(ClipTicket, ClipOutcomeHandle), ServerError> {
        let size = offer.bytes.len();
        if size > MAX_CLIP_BYTES {
            tracing::warn!(
                owner = owner.as_str(),
                bytes = size,
                ceiling = MAX_CLIP_BYTES,
                "audio clip refused: larger than the per-clip ceiling"
            );
            return Err(ServerError::ClipTooLarge {
                bytes: size,
                ceiling: MAX_CLIP_BYTES,
            });
        }

        let now = Instant::now();
        let capability = mint_capability();
        let (outcome_tx, outcome_rx) = oneshot::channel();

        let arm_sweeper = {
            let mut inner = self.lock();
            sweep_locked(&mut inner, now);

            if inner.pending_bytes + size > MAX_TOTAL_CLIP_BYTES {
                tracing::warn!(
                    owner = owner.as_str(),
                    bytes = size,
                    held = inner.pending_bytes,
                    budget = MAX_TOTAL_CLIP_BYTES,
                    "audio clip refused: the clip budget is full"
                );
                return Err(ServerError::ClipBudgetExhausted {
                    bytes: size,
                    budget: MAX_TOTAL_CLIP_BYTES,
                });
            }

            inner.pending_bytes += size;
            inner.entries.insert(
                capability.clone(),
                ClipEntry {
                    owner: owner.clone(),
                    duration_ms: offer.duration_ms,
                    deadline: now + FETCH_WINDOW,
                    expires_by: now + MAX_CLIP_LIFETIME,
                    held_remaining: None,
                    payload: Some(ClipPayload {
                        bytes: Bytes::from(offer.bytes),
                        media_type: offer.media_type,
                    }),
                    fetch_budget: MIN_FETCH_BUDGET,
                    fetches: 0,
                    refusals: 0,
                    first_refusal: None,
                    outcome: outcome_tx,
                },
            );

            let arm = !inner.sweeping;
            inner.sweeping = true;
            arm
        };

        if arm_sweeper {
            self.spawn_sweeper();
        } else {
            self.deadline_moved.notify_one();
        }

        Ok((
            ClipTicket {
                clip_path: clip_path(&capability),
                report_path: report_path(&capability),
                capability: ClipCapability(capability),
            },
            ClipOutcomeHandle(outcome_rx),
        ))
    }

    /// Serves one fetch per admitted page; the bytes stay held until the last admitted fetch.
    pub(crate) fn take_clip(&self, capability: &str) -> Option<ClipPayload> {
        let now = Instant::now();
        let mut inner = self.lock();
        sweep_locked(&mut inner, now);

        let entry = inner.entries.get_mut(capability)?;
        if entry.fetches >= entry.fetch_budget {
            return None;
        }
        let payload = entry.payload.clone()?;
        let answer_by = (now + verdict_window(entry.duration_ms)).min(entry.expires_by);
        let first_fetch = entry.fetches == 0;
        entry.fetches += 1;
        match entry.held_remaining.as_mut() {
            Some(remaining) => *remaining = verdict_window(entry.duration_ms),
            None if first_fetch => entry.deadline = answer_by,
            None => entry.deadline = entry.deadline.max(answer_by),
        }
        drop_spent_payload(&mut inner, capability);
        drop(inner);
        self.deadline_moved.notify_one();
        Some(payload)
    }

    /// The live page count at announcement bounds both the fetches and the verdicts one clip
    /// accepts; it never drops below the fetches already served.
    pub(crate) fn admit_players(&self, capability: &str, players: usize) -> bool {
        let mut inner = self.lock();
        let Some(entry) = inner.entries.get_mut(capability) else {
            return false;
        };
        entry.fetch_budget = players.max(entry.fetches).max(MIN_FETCH_BUDGET);
        drop_spent_payload(&mut inner, capability);
        settle_if_every_page_refused(&mut inner, capability);
        true
    }

    /// Played by any page settles the clip at once; it is refused only once every admitted
    /// page has refused, and then carries the first reason given.
    pub(crate) fn record_verdict(&self, capability: &str, outcome: ClipOutcome) -> bool {
        let mut inner = self.lock();
        sweep_locked(&mut inner, Instant::now());
        let ClipOutcome::Refused { reason } = outcome else {
            return finish_locked(&mut inner, capability, outcome);
        };
        let Some(entry) = inner.entries.get_mut(capability) else {
            return false;
        };
        entry.refusals += 1;
        entry.first_refusal.get_or_insert(reason);
        settle_if_every_page_refused(&mut inner, capability);
        true
    }

    /// A held clip's verdict window stops counting down, but it never outlives its lifetime.
    pub(crate) fn hold(&self, capability: &str) -> bool {
        let now = Instant::now();
        let mut inner = self.lock();
        let Some(entry) = inner.entries.get_mut(capability) else {
            return false;
        };
        if entry.held_remaining.is_none() {
            entry.held_remaining = Some(entry.deadline.saturating_duration_since(now));
            entry.deadline = entry.expires_by;
        }
        drop(inner);
        self.deadline_moved.notify_one();
        true
    }

    pub(crate) fn release_hold(&self, capability: &str) -> bool {
        let now = Instant::now();
        let mut inner = self.lock();
        let Some(entry) = inner.entries.get_mut(capability) else {
            return false;
        };
        if let Some(remaining) = entry.held_remaining.take() {
            entry.deadline = (now + remaining).min(entry.expires_by);
        }
        drop(inner);
        self.deadline_moved.notify_one();
        true
    }

    pub(crate) fn revoke(&self, capability: &str) -> bool {
        let mut inner = self.lock();
        finish_locked(&mut inner, capability, ClipOutcome::Revoked)
    }

    pub(crate) fn discard_all(&self) -> usize {
        let mut inner = self.lock();
        let capabilities: Vec<String> = inner.entries.keys().cloned().collect();
        let discarded = capabilities.len();
        for capability in capabilities {
            finish_locked(&mut inner, &capability, ClipOutcome::ServerStopped);
        }
        if discarded > 0 {
            tracing::debug!(discarded, "audio clips dropped with the listener");
        }
        discarded
    }

    fn spawn_sweeper(self: &Arc<Self>) {
        let store = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                let next = {
                    let mut inner = store.lock();
                    sweep_locked(&mut inner, Instant::now());
                    let next = next_deadline_locked(&inner);
                    if next.is_none() {
                        inner.sweeping = false;
                    }
                    next
                };
                let Some(deadline) = next else {
                    break;
                };
                tokio::select! {
                    () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {}
                    () = store.deadline_moved.notified() => {}
                }
            }
        });
    }
}

fn finish_locked(inner: &mut StoreInner, capability: &str, outcome: ClipOutcome) -> bool {
    let Some(entry) = inner.entries.remove(capability) else {
        return false;
    };
    release_bytes(inner, &entry);
    let _ = entry.outcome.send(outcome);
    true
}

fn drop_spent_payload(inner: &mut StoreInner, capability: &str) {
    let Some(entry) = inner.entries.get_mut(capability) else {
        return;
    };
    if entry.fetches < entry.fetch_budget {
        return;
    }
    if let Some(payload) = entry.payload.take() {
        inner.pending_bytes = inner.pending_bytes.saturating_sub(payload.bytes.len());
    }
}

fn settle_if_every_page_refused(inner: &mut StoreInner, capability: &str) {
    let Some(entry) = inner.entries.get_mut(capability) else {
        return;
    };
    if entry.refusals == 0 || entry.refusals < entry.fetch_budget {
        return;
    }
    let reason = entry.first_refusal.take().unwrap_or_default();
    finish_locked(inner, capability, ClipOutcome::Refused { reason });
}

fn sweep_locked(inner: &mut StoreInner, now: Instant) {
    let expired: Vec<String> = inner
        .entries
        .iter()
        .filter(|(_, entry)| entry.deadline <= now)
        .map(|(capability, _)| capability.clone())
        .collect();

    for capability in expired {
        let Some(entry) = inner.entries.remove(&capability) else {
            continue;
        };
        release_bytes(inner, &entry);
        let outcome = if entry.fetches == 0 {
            ClipOutcome::NeverFetched
        } else {
            ClipOutcome::NoVerdict
        };
        tracing::debug!(
            owner = entry.owner.as_str(),
            fetches = entry.fetches,
            "audio clip window closed"
        );
        let _ = entry.outcome.send(outcome);
    }
}

fn next_deadline_locked(inner: &StoreInner) -> Option<Instant> {
    inner.entries.values().map(|entry| entry.deadline).min()
}

fn release_bytes(inner: &mut StoreInner, entry: &ClipEntry) {
    if let Some(payload) = entry.payload.as_ref() {
        inner.pending_bytes = inner.pending_bytes.saturating_sub(payload.bytes.len());
    }
}

fn verdict_window(duration_ms: u64) -> Duration {
    Duration::from_millis(duration_ms)
        .saturating_add(VERDICT_SLACK)
        .min(MAX_VERDICT_WINDOW)
}

fn mint_capability() -> String {
    let mut bytes = [0u8; CAPABILITY_BYTE_LEN];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::collections::HashSet;

    use tracing::Level;

    use super::*;
    use crate::test_helpers::log_capture;

    const OWNER: &str = "audio-player";
    const CLIP_BYTES: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
    const CLIP_DURATION_MS: u64 = 1_200;
    const SAMPLE_CLIP_BYTES: usize = 4_096;
    const ONE_BYTE: usize = 1;
    const MINT_SAMPLES: usize = 64;
    const SCHEDULER_TURNS: usize = 8;
    const UNKNOWN_CAPABILITY: &str = "SMNhcGFiaWxpdHlUaGF0V2FzTmV2ZXJNaW50ZWRBdEFsbA";
    const MALFORMED_CAPABILITY: &str = "~~not-base64~~";
    const LATE_REASON: &str = "a verdict that arrives after the clip is gone";
    const WELL_PAST_EVERY_WINDOW: Duration = MAX_VERDICT_WINDOW
        .saturating_add(FETCH_WINDOW)
        .saturating_add(VERDICT_SLACK);

    fn owner() -> OverlayId {
        OverlayId::new(OWNER)
    }

    fn offer_of(bytes: Vec<u8>, duration_ms: u64) -> ClipOffer {
        ClipOffer {
            bytes,
            media_type: ClipMediaType::Wave,
            duration_ms,
        }
    }

    fn clip_offer() -> ClipOffer {
        offer_of(CLIP_BYTES.to_vec(), CLIP_DURATION_MS)
    }

    fn sized_offer(bytes: usize) -> ClipOffer {
        offer_of(vec![0; bytes], CLIP_DURATION_MS)
    }

    fn held_bytes(store: &AudioClipStore) -> usize {
        store.lock().pending_bytes
    }

    fn sweeper_armed(store: &AudioClipStore) -> bool {
        store.lock().sweeping
    }

    fn close_every_window(store: &AudioClipStore) {
        let mut inner = store.lock();
        sweep_locked(&mut inner, Instant::now() + WELL_PAST_EVERY_WINDOW);
    }

    fn secrets_of(ticket: &ClipTicket) -> Vec<String> {
        vec![
            ticket.capability().expose().to_owned(),
            ticket.clip_path().to_owned(),
            ticket.report_path().to_owned(),
        ]
    }

    #[tokio::test]
    async fn a_capability_carries_a_full_entropy_draw_no_sibling_repeats() {
        let store = AudioClipStore::new();
        let mut minted = HashSet::new();

        for _ in 0..MINT_SAMPLES {
            let (ticket, _outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            let capability = ticket.capability().expose().to_owned();
            let drawn = URL_SAFE_NO_PAD
                .decode(&capability)
                .expect("a capability must be url-safe base64");
            assert_eq!(
                drawn.len(),
                CAPABILITY_BYTE_LEN,
                "a capability must carry the whole entropy draw"
            );
            assert!(
                minted.insert(capability),
                "two offers minted the same capability"
            );
        }
    }

    #[tokio::test]
    async fn neither_a_ticket_nor_a_capability_can_be_debug_printed_into_a_log() {
        let store = AudioClipStore::new();
        let (ticket, _outcome) = store.offer(&owner(), clip_offer()).expect("offer");

        for rendered in [format!("{ticket:?}"), format!("{:?}", ticket.capability())] {
            for secret in secrets_of(&ticket) {
                assert!(!rendered.contains(&secret), "{rendered} hands out {secret}");
            }
        }
    }

    #[tokio::test]
    async fn the_clip_is_handed_over_once_and_no_other_capability_reaches_it() {
        let store = AudioClipStore::new();
        let (ticket, _outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();

        let payload = store
            .take_clip(capability)
            .expect("the first fetch must hand over the clip");
        assert_eq!(payload.bytes, CLIP_BYTES);
        assert_eq!(payload.media_type, ClipMediaType::Wave);

        for (case, candidate) in [
            ("already fetched", capability),
            ("never minted", UNKNOWN_CAPABILITY),
            ("not a capability at all", MALFORMED_CAPABILITY),
            ("empty", ""),
        ] {
            assert!(
                store.take_clip(candidate).is_none(),
                "{case}: the store handed over a clip it should not have"
            );
        }
    }

    #[tokio::test]
    async fn a_fetch_frees_the_clip_bytes_before_any_verdict_arrives() {
        let store = AudioClipStore::new();
        let (ticket, _outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");
        assert_eq!(held_bytes(&store), SAMPLE_CLIP_BYTES);

        store
            .take_clip(ticket.capability().expose())
            .expect("fetch");

        assert_eq!(
            held_bytes(&store),
            0,
            "only unfetched bytes may count against the budget"
        );
    }

    #[tokio::test]
    async fn the_per_clip_ceiling_admits_the_largest_clip_and_refuses_one_byte_more() {
        for (case, size, admitted) in [
            ("exactly at the ceiling", MAX_CLIP_BYTES, true),
            (
                "one byte over the ceiling",
                MAX_CLIP_BYTES + ONE_BYTE,
                false,
            ),
        ] {
            let store = AudioClipStore::new();
            let outcome = store.offer(&owner(), sized_offer(size));

            match outcome {
                Ok(_) => assert!(
                    admitted,
                    "{case}: the ceiling let an oversized clip through"
                ),
                Err(error) => {
                    assert!(!admitted, "{case}: the ceiling refused a clip that fits");
                    assert!(
                        matches!(
                            error,
                            ServerError::ClipTooLarge { bytes, ceiling }
                                if bytes == size && ceiling == MAX_CLIP_BYTES
                        ),
                        "{case}: refused as {error}"
                    );
                }
            }

            assert_eq!(
                held_bytes(&store),
                if admitted { size } else { 0 },
                "{case}: a refusal must reserve nothing"
            );
        }
    }

    #[tokio::test]
    async fn a_full_budget_refuses_the_next_clip_and_evicts_nothing() {
        let store = AudioClipStore::new();
        let mut admitted = 0;
        while held_bytes(&store) + MAX_CLIP_BYTES <= MAX_TOTAL_CLIP_BYTES {
            store
                .offer(&owner(), sized_offer(MAX_CLIP_BYTES))
                .expect("a clip the budget still has room for");
            admitted += 1;
        }
        assert_eq!(
            held_bytes(&store),
            MAX_TOTAL_CLIP_BYTES,
            "the budget must fill exactly"
        );

        let Err(error) = store.offer(&owner(), sized_offer(ONE_BYTE)) else {
            panic!("a full budget must refuse");
        };

        assert!(
            matches!(
                error,
                ServerError::ClipBudgetExhausted { bytes, budget }
                    if bytes == ONE_BYTE && budget == MAX_TOTAL_CLIP_BYTES
            ),
            "refused as {error}"
        );
        assert_eq!(
            store.lock().entries.len(),
            admitted,
            "no live clip may be evicted to make room for a newer one"
        );
        assert_eq!(held_bytes(&store), MAX_TOTAL_CLIP_BYTES);
    }

    #[tokio::test]
    async fn a_closed_window_tells_a_clip_that_never_arrived_from_one_that_never_answered() {
        for (case, fetched, expected) in [
            ("the page never asked", false, ClipOutcome::NeverFetched),
            (
                "the page fetched and went quiet",
                true,
                ClipOutcome::NoVerdict,
            ),
        ] {
            let store = AudioClipStore::new();
            let (ticket, outcome) = store
                .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
                .expect("offer");
            if fetched {
                store
                    .take_clip(ticket.capability().expose())
                    .expect("fetch");
            }

            close_every_window(&store);

            assert_eq!(outcome.recv().await, expected, "{case}");
            assert_eq!(
                held_bytes(&store),
                0,
                "{case}: a closed window must free the clip bytes"
            );
        }
    }

    #[test]
    fn the_verdict_window_is_the_clip_duration_plus_slack_up_to_a_hard_ceiling() {
        let at_ceiling_ms = u64::try_from((MAX_VERDICT_WINDOW - VERDICT_SLACK).as_millis())
            .expect("the ceiling fits a duration hint");

        for (case, duration_ms, expected) in [
            ("no duration hint", 0, VERDICT_SLACK),
            (
                "a short clip",
                CLIP_DURATION_MS,
                Duration::from_millis(CLIP_DURATION_MS) + VERDICT_SLACK,
            ),
            (
                "one millisecond under the ceiling",
                at_ceiling_ms - 1,
                MAX_VERDICT_WINDOW - Duration::from_millis(1),
            ),
            ("exactly at the ceiling", at_ceiling_ms, MAX_VERDICT_WINDOW),
            (
                "a duration no clip could have",
                u64::MAX,
                MAX_VERDICT_WINDOW,
            ),
        ] {
            assert_eq!(verdict_window(duration_ms), expected, "{case}");
        }
    }

    #[tokio::test]
    async fn a_duration_hint_that_would_overflow_the_clock_still_yields_its_clip() {
        let store = AudioClipStore::new();
        let (ticket, _outcome) = store
            .offer(&owner(), offer_of(CLIP_BYTES.to_vec(), u64::MAX))
            .expect("offer");

        assert!(
            store.take_clip(ticket.capability().expose()).is_some(),
            "an absurd duration hint must not take the fetch down with it"
        );
    }

    #[tokio::test]
    async fn a_verdict_resolves_the_clip_once_and_a_later_one_changes_nothing() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();
        store.take_clip(capability).expect("fetch");

        assert!(store.record_verdict(capability, ClipOutcome::Played));
        assert!(
            !store.record_verdict(
                capability,
                ClipOutcome::Refused {
                    reason: LATE_REASON.to_owned()
                }
            ),
            "a spent capability must not accept a second verdict"
        );

        assert_eq!(outcome.recv().await, ClipOutcome::Played);
    }

    #[tokio::test]
    async fn a_verdict_is_accepted_for_a_clip_whose_bytes_were_never_fetched() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");

        assert!(store.record_verdict(ticket.capability().expose(), ClipOutcome::Played));

        assert_eq!(
            outcome.recv().await,
            ClipOutcome::Played,
            "the store takes the page's word for it without ever handing over the bytes"
        );
        assert_eq!(held_bytes(&store), 0);
    }

    #[tokio::test]
    async fn revoking_a_clip_resolves_it_and_leaves_nothing_to_fetch() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");
        let capability = ticket.capability().expose();

        assert!(store.revoke(capability));
        assert!(store.take_clip(capability).is_none());
        assert!(!store.revoke(capability), "a clip can only be revoked once");

        assert_eq!(outcome.recv().await, ClipOutcome::Revoked);
        assert_eq!(held_bytes(&store), 0);
    }

    #[tokio::test]
    async fn discarding_the_store_resolves_every_clip_fetched_or_not() {
        let store = AudioClipStore::new();
        let (fetched, fetched_outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");
        let (untouched, untouched_outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");
        store
            .take_clip(fetched.capability().expose())
            .expect("fetch");

        assert_eq!(store.discard_all(), 2);

        assert_eq!(fetched_outcome.recv().await, ClipOutcome::ServerStopped);
        assert_eq!(untouched_outcome.recv().await, ClipOutcome::ServerStopped);
        assert!(store.take_clip(untouched.capability().expose()).is_none());
        assert_eq!(held_bytes(&store), 0);
        assert_eq!(store.discard_all(), 0);
    }

    #[test]
    fn a_clip_outcome_resolves_when_the_runtime_holding_the_store_goes_away() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let outcome = runtime.block_on(async {
            let store = AudioClipStore::new();
            let (_ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            outcome
        });

        drop(runtime);

        let waiter = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        assert_eq!(
            waiter.block_on(outcome.recv()),
            ClipOutcome::ServerStopped,
            "a caller must never hang on a store that is gone"
        );
    }

    #[tokio::test]
    async fn the_sweeper_arms_on_the_first_clip_retires_when_the_store_empties_and_arms_again() {
        let store = AudioClipStore::new();
        assert!(!sweeper_armed(&store), "an idle store runs no sweeper");

        let (first, _first_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        assert!(sweeper_armed(&store), "the first clip must arm the sweeper");
        let (second, _second_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        assert!(sweeper_armed(&store));

        assert!(store.revoke(first.capability().expose()));
        assert!(store.revoke(second.capability().expose()));
        store.deadline_moved.notify_one();
        for _ in 0..SCHEDULER_TURNS {
            tokio::task::yield_now().await;
        }
        assert!(
            !sweeper_armed(&store),
            "a sweeper must retire rather than poll an empty store"
        );

        let (_third, _third_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        assert!(
            sweeper_armed(&store),
            "a clip arriving after the sweeper retired must arm a new one"
        );
    }

    #[tokio::test]
    async fn the_next_sweep_is_scheduled_for_the_clip_whose_window_closes_first() {
        let store = AudioClipStore::new();
        let (_later, _later_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let (sooner, _sooner_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        store
            .take_clip(sooner.capability().expose())
            .expect("fetch");

        let inner = store.lock();
        let next = next_deadline_locked(&inner).expect("a live store must schedule a sweep");

        assert_eq!(
            next,
            inner.entries[sooner.capability().expose()].deadline,
            "a clip whose window closes first must not wait on a later one"
        );
    }

    const FIRST_REFUSAL: &str = "the second scene's page is muted";
    const SECOND_REFUSAL: &str = "the third scene's page is muted";
    const ONE_SECOND: Duration = Duration::from_secs(1);

    fn refused(reason: &str) -> ClipOutcome {
        ClipOutcome::Refused {
            reason: reason.to_owned(),
        }
    }

    fn still_waiting(store: &AudioClipStore, capability: &str) -> bool {
        store.lock().entries.contains_key(capability)
    }

    fn sweep_at(store: &AudioClipStore, at: Instant) {
        let mut inner = store.lock();
        sweep_locked(&mut inner, at);
    }

    #[tokio::test]
    async fn each_admitted_page_gets_one_fetch_and_the_next_one_misses() {
        for (admitted, served) in [(0, 1), (1, 1), (2, 2), (3, 3)] {
            let store = AudioClipStore::new();
            let (ticket, _outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            let capability = ticket.capability().expose();
            assert!(store.admit_players(capability, admitted));

            let fetched = (0..=served)
                .take_while(|_| store.take_clip(capability).is_some())
                .count();

            assert_eq!(fetched, served, "{admitted} admitted pages");
        }
    }

    #[tokio::test]
    async fn the_clip_bytes_stay_held_until_the_last_admitted_page_fetched() {
        let store = AudioClipStore::new();
        let (ticket, _outcome) = store
            .offer(&owner(), sized_offer(SAMPLE_CLIP_BYTES))
            .expect("offer");
        let capability = ticket.capability().expose();
        store.admit_players(capability, 2);

        let first = store.take_clip(capability).expect("first page fetch");
        let held_after_first = held_bytes(&store);
        let second = store.take_clip(capability).expect("second page fetch");

        assert_eq!(first.bytes, second.bytes, "both pages get the same clip");
        assert_eq!(held_after_first, SAMPLE_CLIP_BYTES);
        assert_eq!(held_bytes(&store), 0, "the last fetch frees the bytes");
    }

    #[tokio::test]
    async fn verdicts_from_several_pages_settle_the_clip_the_way_the_route_promises() {
        for (case, admitted, verdicts, expected) in [
            (
                "a refusal from one page does not undo another page playing",
                2,
                vec![refused(FIRST_REFUSAL), ClipOutcome::Played],
                ClipOutcome::Played,
            ),
            (
                "every page refused, first reason kept",
                2,
                vec![refused(FIRST_REFUSAL), refused(SECOND_REFUSAL)],
                refused(FIRST_REFUSAL),
            ),
            (
                "one page refused and the other went quiet",
                2,
                vec![refused(FIRST_REFUSAL)],
                ClipOutcome::NoVerdict,
            ),
            (
                "the only page refused",
                1,
                vec![refused(FIRST_REFUSAL)],
                refused(FIRST_REFUSAL),
            ),
        ] {
            let store = AudioClipStore::new();
            let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            let capability = ticket.capability().expose();
            store.admit_players(capability, admitted);
            for _ in 0..admitted {
                store.take_clip(capability).expect("fetch");
            }

            for verdict in verdicts {
                assert!(store.record_verdict(capability, verdict), "{case}");
            }
            close_every_window(&store);

            assert_eq!(outcome.recv().await, expected, "{case}");
        }
    }

    #[tokio::test]
    async fn readmitting_below_the_fetches_already_served_keeps_every_served_page_admitted() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();
        store.admit_players(capability, 3);
        store.take_clip(capability).expect("first fetch");
        store.take_clip(capability).expect("second fetch");

        store.admit_players(capability, 1);
        store.record_verdict(capability, refused(FIRST_REFUSAL));
        let waiting_after_one_refusal = still_waiting(&store, capability);
        store.record_verdict(capability, refused(SECOND_REFUSAL));

        assert!(
            waiting_after_one_refusal,
            "one refusal settled a clip two pages were playing"
        );
        assert_eq!(outcome.recv().await, refused(FIRST_REFUSAL));
    }

    #[tokio::test]
    async fn admitting_fewer_pages_after_every_served_page_refused_settles_the_clip() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();
        store.admit_players(capability, 3);
        store.take_clip(capability).expect("first fetch");
        store.take_clip(capability).expect("second fetch");
        store.record_verdict(capability, refused(FIRST_REFUSAL));
        store.record_verdict(capability, refused(SECOND_REFUSAL));

        store.admit_players(capability, 2);

        assert!(
            !still_waiting(&store, capability),
            "the clip kept waiting on a page that was never there"
        );
        assert_eq!(outcome.recv().await, refused(FIRST_REFUSAL));
    }

    #[tokio::test]
    async fn a_held_clip_outlasts_its_verdict_window_and_settles_on_the_page_verdict() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();
        store.take_clip(capability).expect("fetch");

        assert!(store.hold(capability));
        sweep_at(
            &store,
            Instant::now() + verdict_window(CLIP_DURATION_MS) + ONE_SECOND,
        );
        assert!(store.release_hold(capability));
        store.record_verdict(capability, refused(FIRST_REFUSAL));

        assert_eq!(
            outcome.recv().await,
            refused(FIRST_REFUSAL),
            "a paused clip must not be swept as unanswered"
        );
    }

    #[tokio::test]
    async fn a_hold_that_runs_to_the_lifetime_cap_still_expires() {
        let store = AudioClipStore::new();
        let (ticket, outcome) = store.offer(&owner(), clip_offer()).expect("offer");
        let capability = ticket.capability().expose();
        store.take_clip(capability).expect("fetch");
        store.hold(capability);

        sweep_at(&store, Instant::now() + MAX_CLIP_LIFETIME - ONE_SECOND);
        let held_before_the_cap = still_waiting(&store, capability);
        sweep_at(&store, Instant::now() + MAX_CLIP_LIFETIME);
        assert!(
            !still_waiting(&store, capability),
            "a hold kept the clip past its lifetime cap"
        );

        assert!(
            held_before_the_cap,
            "the hold expired before the lifetime cap"
        );
        assert_eq!(outcome.recv().await, ClipOutcome::NoVerdict);
    }

    #[test]
    fn no_store_event_at_any_level_names_a_capability_or_a_clip_address() {
        let (secrets, lines) = log_capture::capture_blocking(Level::TRACE, async {
            let store = AudioClipStore::new();
            let mut secrets = Vec::new();

            let (played, _played_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            secrets.extend(secrets_of(&played));
            store
                .take_clip(played.capability().expose())
                .expect("fetch");
            store.record_verdict(played.capability().expose(), ClipOutcome::Played);

            let (revoked, _revoked_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            secrets.extend(secrets_of(&revoked));
            store.revoke(revoked.capability().expose());

            let (expired, _expired_outcome) = store.offer(&owner(), clip_offer()).expect("offer");
            secrets.extend(secrets_of(&expired));
            close_every_window(&store);

            let (discarded, _discarded_outcome) =
                store.offer(&owner(), clip_offer()).expect("offer");
            secrets.extend(secrets_of(&discarded));
            store.discard_all();

            assert!(
                store
                    .offer(&owner(), sized_offer(MAX_CLIP_BYTES + ONE_BYTE))
                    .is_err(),
                "an oversized clip must be refused"
            );

            secrets
        });

        assert!(
            lines.iter().any(log_capture::CapturedLine::from_forge),
            "the capture saw none of the store's own lines, so it proves nothing"
        );
        for secret in &secrets {
            assert!(
                !lines.iter().any(|line| line.mentions(secret)),
                "a store event named {secret}"
            );
        }
    }
}
