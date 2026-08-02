use forge_speak_queue::{RequestId, SpeakEvent};

#[derive(Clone)]
pub struct NowSpeaking {
    pub request_id: RequestId,
    pub viewer_name: String,
    pub engine_id: String,
    pub voice_id: String,
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
    manual_paused: bool,
    gate_held: bool,
    now_speaking: Option<NowSpeaking>,
    queue: Vec<QueueItem>,
    stats: SessionStats,
    last_drop: Option<String>,
}

impl SpeakState {
    pub fn new() -> Self {
        Self {
            manual_paused: false,
            gate_held: false,
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
                voice_preview,
                estimated_secs,
                ..
            } => {
                self.queue.push(QueueItem {
                    request_id,
                    viewer_name,
                    engine_voice: voice_preview,
                    text,
                    duration_secs: estimated_secs,
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
                    request_id,
                    viewer_name,
                    engine_id: engine_id.0,
                    voice_id: voice_id.0,
                    text,
                    elapsed_secs: 0,
                    total_secs: duration_secs,
                });
                true
            }
            SpeakEvent::Progress {
                request_id,
                elapsed_secs,
            } => match self.now_speaking.as_mut() {
                Some(ns) if ns.request_id == request_id => {
                    ns.elapsed_secs = elapsed_secs;
                    true
                }
                _ => false,
            },
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
            SpeakEvent::Removed { request_id } => {
                let before = self.queue.len();
                self.queue.retain(|item| item.request_id != request_id);
                self.queue.len() != before
            }
            SpeakEvent::Rejected { .. } => {
                self.stats.filtered = self.stats.filtered.saturating_add(1);
                true
            }
            SpeakEvent::QueueChanged { .. } => false,
            SpeakEvent::Paused { .. } => {
                let changed = !self.manual_paused;
                self.manual_paused = true;
                changed
            }
            SpeakEvent::Resumed => {
                let changed = self.manual_paused;
                self.manual_paused = false;
                changed
            }
            SpeakEvent::VoiceGateHeld => {
                let changed = !self.gate_held;
                self.gate_held = true;
                changed
            }
            SpeakEvent::VoiceGateReleased => {
                let changed = self.gate_held;
                self.gate_held = false;
                changed
            }
            SpeakEvent::Cleared => {
                self.queue.clear();
                self.now_speaking = None;
                true
            }
        }
    }

    /// Optimistic: the confirming `Paused`/`Resumed` event re-seats the actual state.
    pub fn set_manual_paused(&mut self, paused: bool) {
        self.manual_paused = paused;
    }

    /// Optimistic: the confirming `Cleared` event re-seats the actual state.
    pub fn clear_all(&mut self) {
        self.now_speaking = None;
        self.queue.clear();
    }

    pub fn manual_paused(&self) -> bool {
        self.manual_paused
    }

    /// Released by the voice gate itself when the microphone goes quiet - not user-clearable.
    pub fn gate_held(&self) -> bool {
        self.gate_held
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

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_pause() -> SpeakEvent {
        SpeakEvent::Paused {
            reason: "user paused".into(),
        }
    }

    #[test]
    fn manual_pause_events_leave_the_voice_gate_hold_untouched() {
        let mut state = SpeakState::new();
        state.apply_event(SpeakEvent::VoiceGateHeld);

        state.apply_event(manual_pause());
        assert!(state.gate_held());
        assert!(state.manual_paused());

        state.apply_event(SpeakEvent::Resumed);
        assert!(
            state.gate_held(),
            "clicking resume must not hide an active gate hold"
        );
        assert!(!state.manual_paused());
    }

    #[test]
    fn voice_gate_events_leave_the_manual_pause_untouched() {
        let mut state = SpeakState::new();
        state.apply_event(manual_pause());

        state.apply_event(SpeakEvent::VoiceGateHeld);
        assert!(state.manual_paused());
        assert!(state.gate_held());

        state.apply_event(SpeakEvent::VoiceGateReleased);
        assert!(
            state.manual_paused(),
            "the gate going quiet must not un-pause the queue"
        );
        assert!(!state.gate_held());
    }

    #[test]
    fn apply_event_reports_change_only_when_a_pause_flag_flips() {
        for (case, prime, probe, expected) in [
            ("first manual pause", vec![], manual_pause(), true),
            (
                "repeated manual pause",
                vec![manual_pause()],
                manual_pause(),
                false,
            ),
            (
                "resume after pause",
                vec![manual_pause()],
                SpeakEvent::Resumed,
                true,
            ),
            ("resume while running", vec![], SpeakEvent::Resumed, false),
            ("first gate hold", vec![], SpeakEvent::VoiceGateHeld, true),
            (
                "repeated gate hold",
                vec![SpeakEvent::VoiceGateHeld],
                SpeakEvent::VoiceGateHeld,
                false,
            ),
            (
                "release after hold",
                vec![SpeakEvent::VoiceGateHeld],
                SpeakEvent::VoiceGateReleased,
                true,
            ),
            (
                "release while open",
                vec![],
                SpeakEvent::VoiceGateReleased,
                false,
            ),
        ] {
            let mut state = SpeakState::new();
            for event in prime {
                state.apply_event(event);
            }
            assert_eq!(state.apply_event(probe), expected, "{case}");
        }
    }
}
