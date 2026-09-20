use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

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

pub(crate) struct ClipPayload {
    pub(crate) bytes: Vec<u8>,
    pub(crate) media_type: ClipMediaType,
}

struct ClipEntry {
    owner: OverlayId,
    duration_ms: u64,
    deadline: Instant,
    payload: Option<ClipPayload>,
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
                    payload: Some(ClipPayload {
                        bytes: offer.bytes,
                        media_type: offer.media_type,
                    }),
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

    pub(crate) fn take_clip(&self, capability: &str) -> Option<ClipPayload> {
        let now = Instant::now();
        let mut inner = self.lock();
        sweep_locked(&mut inner, now);

        let payload = {
            let entry = inner.entries.get_mut(capability)?;
            let payload = entry.payload.take()?;
            entry.deadline = now + verdict_window(entry.duration_ms);
            payload
        };
        inner.pending_bytes = inner.pending_bytes.saturating_sub(payload.bytes.len());
        drop(inner);
        self.deadline_moved.notify_one();
        Some(payload)
    }

    pub(crate) fn record_verdict(&self, capability: &str, outcome: ClipOutcome) -> bool {
        let mut inner = self.lock();
        sweep_locked(&mut inner, Instant::now());
        finish_locked(&mut inner, capability, outcome)
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
        let outcome = if entry.payload.is_some() {
            ClipOutcome::NeverFetched
        } else {
            ClipOutcome::NoVerdict
        };
        tracing::debug!(
            owner = entry.owner.as_str(),
            fetched = entry.payload.is_none(),
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
