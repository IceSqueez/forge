use forge_speak_queue::{RequestId, SpeakEvent};

/// The utterance the queue is currently voicing — a cached read of the speak
/// queue's now-playing slot, folded in from the `SpeakEvent` bridge. Cleared when
/// the active item finishes, fails, is skipped, or the queue is cleared.
#[derive(Clone)]
pub struct NowSpeaking {
    pub viewer_name: String,
    pub engine_voice: String,
    pub text: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
}

/// One pending utterance in the up-next queue — a cached view-model of an enqueued
/// speak request, keyed by its `request_id` so the `Started` event can lift the
/// matching row out of the queue into the now-speaking slot.
#[derive(Clone)]
pub struct QueueItem {
    pub request_id: RequestId,
    pub viewer_name: String,
    pub engine_voice: String,
    pub text: String,
    pub duration_secs: u32,
    pub is_high_priority: bool,
    pub bits_amount: Option<u32>,
}

/// The session counters shown in the dashboard's right rail, accumulated over the
/// `SpeakEvent` stream for the app's lifetime — spoken/skipped/filtered tallies and
/// the average synthesis latency (`None` until the queue reports one).
#[derive(Clone)]
pub struct SessionStats {
    pub spoken: u32,
    pub skipped: u32,
    pub filtered: u32,
    pub avg_latency_ms: Option<u32>,
}

/// Boot-global observable cache of the speak queue's live state, fed by the runtime→UI
/// bridge draining the queue's `SpeakEvent` stream. It holds the now-speaking slot, the
/// up-next queue, the paused flag and the session counters — the source of truth stays
/// in `forge-speak-queue`; this is a cached read the bridge advances. It lives in the
/// [`crate::topics::Topics`] bundle so the counters keep accumulating even when the TTS
/// dashboard isn't mounted; the observing [`crate::tts_dashboard::TtsDashboardView`]
/// repaints when the bridge `cx.notify()`s it.
pub struct SpeakState {
    paused: bool,
    now_speaking: Option<NowSpeaking>,
    queue: Vec<QueueItem>,
    stats: SessionStats,
}

impl SpeakState {
    /// An empty, live-fed cache: nothing speaking, an empty queue, all counters at
    /// zero and no latency reported yet. The bridge fills it in over the `SpeakEvent`
    /// stream.
    pub fn new() -> Self {
        Self {
            paused: false,
            now_speaking: None,
            queue: Vec::new(),
            stats: SessionStats {
                spoken: 0,
                skipped: 0,
                filtered: 0,
                avg_latency_ms: None,
            },
        }
    }

    /// Folds one `SpeakEvent` into the cache, mirroring the queue's own lifecycle:
    /// `Enqueued` appends a pending row, `Started` lifts the matching row into the
    /// now-speaking slot, and the terminal kinds (`Finished`/`Failed`/`Skipped`) clear
    /// the slot and advance the session counters. Reports whether anything actually
    /// changed so the bridge only repaints on a real update. Kept free of `cx` so it
    /// stays directly exercisable.
    pub fn apply_event(&mut self, event: SpeakEvent) -> bool {
        match event {
            SpeakEvent::Enqueued {
                request_id,
                viewer_name,
                text,
                is_high_priority,
                ..
            } => {
                self.queue.push(QueueItem {
                    request_id,
                    viewer_name,
                    engine_voice: String::new(),
                    text,
                    duration_secs: 0,
                    is_high_priority,
                    bits_amount: None,
                });
                true
            }
            SpeakEvent::Started {
                request_id,
                voice_id,
                engine_id,
                viewer_name,
                text,
                duration_secs,
            } => {
                self.queue.retain(|item| item.request_id != request_id);
                self.now_speaking = Some(NowSpeaking {
                    viewer_name,
                    engine_voice: if voice_id.0.is_empty() {
                        String::new()
                    } else {
                        format!("{}/{}", engine_id.0, voice_id.0)
                    },
                    text,
                    elapsed_secs: 0,
                    total_secs: duration_secs,
                });
                true
            }
            SpeakEvent::Finished { .. } => {
                self.now_speaking = None;
                self.stats.spoken = self.stats.spoken.saturating_add(1);
                true
            }
            SpeakEvent::Failed { .. } => {
                self.now_speaking = None;
                true
            }
            SpeakEvent::Skipped { .. } => {
                self.now_speaking = None;
                self.stats.skipped = self.stats.skipped.saturating_add(1);
                true
            }
            SpeakEvent::Rejected { .. } => {
                self.stats.filtered = self.stats.filtered.saturating_add(1);
                true
            }
            SpeakEvent::QueueChanged { .. } => false,
            SpeakEvent::Paused { .. } => {
                self.paused = true;
                true
            }
            SpeakEvent::Resumed => {
                self.paused = false;
                true
            }
            SpeakEvent::Cleared => {
                self.queue.clear();
                self.now_speaking = None;
                true
            }
        }
    }

    /// Optimistically flips the paused flag ahead of the queue's `Paused`/`Resumed`
    /// acknowledgement, so the control strip's button label tracks the intent
    /// immediately. The confirming event re-seats it to the queue's actual state.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Optimistically empties the now-speaking slot and the queue ahead of the queue's
    /// `Cleared` acknowledgement, mirroring the Stop-all control's immediate effect.
    pub fn clear_all(&mut self) {
        self.now_speaking = None;
        self.queue.clear();
    }

    // --- snapshots (owned clones, kept off render for a borrow-free view) -----

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn now_speaking_snapshot(&self) -> Option<NowSpeaking> {
        self.now_speaking.clone()
    }

    pub fn queue_snapshot(&self) -> Vec<QueueItem> {
        self.queue.clone()
    }

    pub fn stats_snapshot(&self) -> SessionStats {
        self.stats.clone()
    }
}
