use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use forge_overlay::silent_reveal_content;
use forge_registry::CancelSignal;
use forge_storage::OverlayId;
use tokio::sync::oneshot;
use tokio::task::{JoinError, JoinHandle};
use tokio::time::Instant;

use crate::overlay_service::{OverlayDelivery, OverlayFrameSink, content_json};
use crate::speak_dispatcher::{ShowSpeech, SpeakDispatchError, SpeakDispatcher, SpeechStartSignal};

/// A memory guard on shows waiting behind the one on screen, not a pacing limit: a backlog below
/// it is always shown in full, however late.
pub const SHOW_QUEUE_CAPACITY: usize = 64;

/// No show holds its overlay longer than this, whatever duration it asked for.
pub const SHOW_CEILING: Duration = Duration::from_secs(120);

/// The page runtime's own wait before a silent reveal must stay longer than this.
pub const SHOW_SPEECH_START_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowEnd {
    Shown(OverlayDelivery),
    Cleared,
    Withdrawn,
}

#[derive(Debug)]
pub struct ShowTicket(oneshot::Receiver<ShowEnd>);

impl ShowTicket {
    /// Resolves once the show has left the screen or left the queue without being shown.
    pub async fn finished(self) -> ShowEnd {
        self.0.await.unwrap_or(ShowEnd::Withdrawn)
    }
}

pub(crate) struct Show {
    pub(crate) content: serde_json::Value,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) window: Duration,
    pub(crate) speech: Option<ShowSpeech>,
}

pub(crate) struct QueueFull;

enum SpeechOpening {
    Started,
    Ended(Result<Result<(), SpeakDispatchError>, JoinError>),
    TooLate,
}

struct Pending {
    show: Show,
    done: oneshot::Sender<ShowEnd>,
}

#[derive(Default)]
struct Lane {
    pending: VecDeque<Pending>,
    running: bool,
}

/// One presenter task per overlay with shows queued; it exits when its queue drains, so idle
/// overlays cost nothing. The map lock is never held across an await.
pub(crate) struct ShowSequencer {
    frames: Option<Arc<dyn OverlayFrameSink>>,
    speaker: Option<Arc<dyn SpeakDispatcher>>,
    lanes: Mutex<HashMap<OverlayId, Lane>>,
}

impl ShowSequencer {
    pub(crate) fn new(
        frames: Option<Arc<dyn OverlayFrameSink>>,
        speaker: Option<Arc<dyn SpeakDispatcher>>,
    ) -> Self {
        Self {
            frames,
            speaker,
            lanes: Mutex::default(),
        }
    }

    pub(crate) fn enqueue(
        self: &Arc<Self>,
        id: &OverlayId,
        show: Show,
    ) -> Result<ShowTicket, QueueFull> {
        let (done, ticket) = oneshot::channel();
        let start_presenter = {
            let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
            let lane = lanes.entry(id.clone()).or_default();
            if lane.pending.len() >= SHOW_QUEUE_CAPACITY {
                return Err(QueueFull);
            }
            lane.pending.push_back(Pending { show, done });
            !std::mem::replace(&mut lane.running, true)
        };
        if start_presenter {
            tokio::spawn(Arc::clone(self).present_all(id.clone()));
        }
        Ok(ShowTicket(ticket))
    }

    pub(crate) fn depth(&self, id: &OverlayId) -> usize {
        self.lanes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .map_or(0, |lane| lane.pending.len())
    }

    /// Drops every show still waiting for `id`, ending each with `end`; the one on screen runs
    /// out its window.
    pub(crate) fn drop_pending(&self, id: &OverlayId, end: ShowEnd) -> usize {
        let dropped: Vec<Pending> = {
            let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
            lanes
                .get_mut(id)
                .map(|lane| lane.pending.drain(..).collect())
                .unwrap_or_default()
        };
        let count = dropped.len();
        for pending in dropped {
            let _ = pending.done.send(end);
        }
        count
    }

    async fn present_all(self: Arc<Self>, id: OverlayId) {
        while let Some(next) = self.next(&id) {
            let delivery = self.present(&id, &next.show).await;
            self.hold(&id, next.show).await;
            let _ = next.done.send(ShowEnd::Shown(delivery));
        }
    }

    /// The page reveals a show when its speech begins, so the window runs from that start; speech
    /// not begun within [`SHOW_SPEECH_START_WAIT`] is withdrawn and the show is shown silently.
    async fn hold(&self, id: &OverlayId, show: Show) {
        let window = show.window.min(SHOW_CEILING);
        let (Some(speaker), Some(speech)) = (self.speaker.clone(), show.speech) else {
            tokio::time::sleep(window).await;
            return;
        };
        let ceiling = Instant::now() + SHOW_CEILING;
        let token = speech.show.clone();
        let cancel = CancelSignal::new();
        let started = SpeechStartSignal::new();
        let mut spoken = tokio::spawn({
            let cancel = cancel.clone();
            let started = started.clone();
            async move { speaker.speak_for_show(speech, cancel, started).await }
        });
        let opening = tokio::select! {
            biased;
            () = started.started() => SpeechOpening::Started,
            outcome = &mut spoken => SpeechOpening::Ended(outcome),
            () = tokio::time::sleep(SHOW_SPEECH_START_WAIT) => SpeechOpening::TooLate,
        };
        match opening {
            SpeechOpening::Started => {
                hold_with_speech(id, window, ceiling, cancel, spoken).await;
            }
            SpeechOpening::Ended(outcome) => {
                if let Some(reason) = unspoken_reason(outcome) {
                    tracing::info!(
                        overlay = %id,
                        reason = %reason,
                        "show speech never played; the show was shown without it"
                    );
                }
                self.show_silently(id, &token, window, ceiling).await;
            }
            SpeechOpening::TooLate => {
                cancel.cancel();
                let _ = spoken.await;
                tracing::info!(
                    overlay = %id,
                    wait_secs = SHOW_SPEECH_START_WAIT.as_secs(),
                    "show speech did not begin in time, so it was withdrawn and the show was shown without it"
                );
                self.show_silently(id, &token, window, ceiling).await;
            }
        }
    }

    async fn show_silently(&self, id: &OverlayId, show: &str, window: Duration, ceiling: Instant) {
        if let Some(frames) = &self.frames {
            frames
                .deliver_content(id, content_json(&silent_reveal_content(show)), None)
                .await;
        }
        tokio::time::sleep_until((Instant::now() + window).min(ceiling)).await;
    }

    async fn present(&self, id: &OverlayId, show: &Show) -> OverlayDelivery {
        let Some(frames) = &self.frames else {
            return OverlayDelivery::NoPage;
        };
        frames
            .deliver_content(id, show.content.clone(), show.duration_ms)
            .await
            .outcome()
    }

    fn next(&self, id: &OverlayId) -> Option<Pending> {
        let mut lanes = self.lanes.lock().unwrap_or_else(PoisonError::into_inner);
        let lane = lanes.get_mut(id)?;
        let next = lane.pending.pop_front();
        if next.is_none() {
            lanes.remove(id);
        }
        next
    }
}

async fn hold_with_speech(
    id: &OverlayId,
    window: Duration,
    ceiling: Instant,
    cancel: CancelSignal,
    mut spoken: JoinHandle<Result<(), SpeakDispatchError>>,
) {
    let held = async {
        tokio::time::sleep(window).await;
        (&mut spoken).await
    };
    match tokio::time::timeout_at(ceiling, held).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => tracing::info!(
            overlay = %id,
            reason = %e,
            "show speech did not play in full; the show kept its display window"
        ),
        Ok(Err(e)) => {
            tracing::warn!(overlay = %id, error = %e, "show speech task ended abnormally")
        }
        Err(_) => {
            cancel.cancel();
            let _ = spoken.await;
            tracing::info!(
                overlay = %id,
                ceiling_secs = SHOW_CEILING.as_secs(),
                "show speech ran past the show ceiling and was ended with the show"
            );
        }
    }
}

fn unspoken_reason(outcome: Result<Result<(), SpeakDispatchError>, JoinError>) -> Option<String> {
    match outcome {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e.to_string()),
        Err(e) => Some(e.to_string()),
    }
}
