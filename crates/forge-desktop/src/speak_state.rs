use forge_speak_queue::{RequestId, SpeakEvent};

#[derive(Clone)]
pub struct NowSpeaking {
    pub viewer_name: String,
    pub engine_voice: String,
    pub text: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
}

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

#[derive(Clone)]
pub struct SessionStats {
    pub spoken: u32,
    pub skipped: u32,
    pub filtered: u32,
    pub avg_latency_ms: Option<u32>,
}

pub struct SpeakState {
    paused: bool,
    now_speaking: Option<NowSpeaking>,
    queue: Vec<QueueItem>,
    stats: SessionStats,
    last_drop: Option<String>,
}

impl SpeakState {
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
            last_drop: None,
        }
    }

    pub fn last_drop(&self) -> Option<&str> {
        self.last_drop.as_deref()
    }

    /// Returns whether the cache changed, so the bridge repaints only on a real update.
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
                self.last_drop = None;
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
            SpeakEvent::Failed { error, .. } => {
                self.now_speaking = None;
                self.last_drop = Some(error);
                true
            }
            SpeakEvent::Skipped { reason, .. } => {
                self.now_speaking = None;
                self.stats.skipped = self.stats.skipped.saturating_add(1);
                self.last_drop = Some(reason);
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

    /// Optimistic: the confirming `Paused`/`Resumed` event re-seats the actual state.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Optimistic: the confirming `Cleared` event re-seats the actual state.
    pub fn clear_all(&mut self) {
        self.now_speaking = None;
        self.queue.clear();
    }

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
