use forge_speak_queue::{QueuedOrderEntry, RequestId, SpeakEvent};

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
                let item = QueueItem {
                    request_id,
                    viewer_name,
                    engine_voice: voice_preview,
                    text,
                    duration_secs: estimated_secs,
                    is_high_priority,
                    bits_amount: None,
                };
                match self
                    .queue
                    .iter_mut()
                    .find(|queued| queued.request_id == item.request_id)
                {
                    Some(queued) => *queued = item,
                    None => self.queue.push(item),
                }
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
            SpeakEvent::Removed { .. } => false,
            SpeakEvent::Rejected { .. } => {
                self.stats.filtered = self.stats.filtered.saturating_add(1);
                true
            }
            SpeakEvent::QueueChanged { order, .. } => self.reseat_queue(&order),
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
                self.now_speaking = None;
                true
            }
        }
    }

    /// `order` is the authority on membership, sequence and priority; ids with no cached
    /// item (bridge lag) are skipped because the list can only render data it holds.
    fn reseat_queue(&mut self, order: &[QueuedOrderEntry]) -> bool {
        let reseated: Vec<QueueItem> = order
            .iter()
            .filter_map(|entry| {
                self.queue
                    .iter()
                    .find(|item| item.request_id == entry.request_id)
                    .map(|item| QueueItem {
                        is_high_priority: entry.is_high_priority,
                        ..item.clone()
                    })
            })
            .collect();

        let changed = reseated.len() != self.queue.len()
            || reseated.iter().zip(&self.queue).any(|(new, old)| {
                new.request_id != old.request_id || new.is_high_priority != old.is_high_priority
            });
        if changed {
            self.queue = reseated;
        }
        changed
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

    fn rid(id: &str) -> RequestId {
        RequestId(id.to_owned())
    }

    fn enqueued(id: &str, is_high_priority: bool) -> SpeakEvent {
        enqueued_saying(id, is_high_priority, "hello")
    }

    fn enqueued_saying(id: &str, is_high_priority: bool, text: &str) -> SpeakEvent {
        SpeakEvent::Enqueued {
            request_id: rid(id),
            queue_len: 0,
            viewer_name: format!("viewer-{id}"),
            text: text.to_owned(),
            is_high_priority,
            voice_preview: "piper / amy".into(),
            estimated_secs: 3,
        }
    }

    fn queue_changed(order: &[(&str, bool)]) -> SpeakEvent {
        SpeakEvent::QueueChanged {
            queue_len: order.len(),
            order: order
                .iter()
                .map(|(id, is_high_priority)| QueuedOrderEntry {
                    request_id: rid(id),
                    is_high_priority: *is_high_priority,
                })
                .collect(),
        }
    }

    fn started(id: &str) -> SpeakEvent {
        SpeakEvent::Started {
            request_id: rid(id),
            voice_id: forge_tts_core::VoiceId("amy".into()),
            engine_id: forge_tts_core::EngineId("piper".into()),
            viewer_name: format!("viewer-{id}"),
            text: "hello".into(),
            duration_secs: 3,
        }
    }

    fn queue_ids(state: &SpeakState) -> Vec<String> {
        state
            .queue_snapshot()
            .into_iter()
            .map(|item| item.request_id.0)
            .collect()
    }

    fn seeded(ids: &[&str]) -> SpeakState {
        let mut state = SpeakState::new();
        for id in ids {
            state.apply_event(enqueued(id, false));
        }
        state
    }

    #[test]
    fn queue_changed_order_reseats_the_pending_list_into_playback_sequence() {
        let mut state = seeded(&["a", "b", "c"]);

        state.apply_event(queue_changed(&[("c", false), ("a", false), ("b", false)]));

        assert_eq!(queue_ids(&state), ["c", "a", "b"]);
    }

    #[test]
    fn queue_changed_order_is_the_authority_on_queue_membership() {
        for (case, order, expected) in [
            (
                "an id dropped from the order leaves the list",
                vec![("a", false), ("c", false)],
                vec!["a", "c"],
            ),
            (
                "an id with no cached item renders no phantom row",
                vec![("a", false), ("d", false), ("c", false)],
                vec!["a", "c"],
            ),
        ] {
            let mut state = seeded(&["a", "b", "c"]);

            state.apply_event(queue_changed(&order));

            assert_eq!(queue_ids(&state), expected, "{case}");
        }
    }

    #[test]
    fn queue_changed_entry_overrides_the_priority_the_item_was_enqueued_with() {
        for (case, enqueued_high, seated_high) in [
            ("promoted into the high queue", false, true),
            ("demoted into the normal queue", true, false),
        ] {
            let mut state = SpeakState::new();
            state.apply_event(enqueued("a", enqueued_high));

            state.apply_event(queue_changed(&[("a", seated_high)]));

            let flags: Vec<bool> = state
                .queue_snapshot()
                .iter()
                .map(|item| item.is_high_priority)
                .collect();
            assert_eq!(flags, [seated_high], "{case}");
        }
    }

    #[test]
    fn queue_changed_reports_a_repaint_only_when_the_visible_list_moves() {
        for (case, order, expected) in [
            ("identical order", vec![("a", false), ("b", false)], false),
            ("permuted order", vec![("b", false), ("a", false)], true),
            ("priority flip only", vec![("a", true), ("b", false)], true),
            ("shrunken membership", vec![("a", false)], true),
        ] {
            let mut state = seeded(&["a", "b"]);

            assert_eq!(state.apply_event(queue_changed(&order)), expected, "{case}");
        }
    }

    #[test]
    fn repeated_enqueued_for_one_request_replaces_the_row_instead_of_duplicating_it() {
        for (case, between) in [
            ("back to back", Vec::new()),
            (
                "after the order was already reseated",
                vec![queue_changed(&[("a", false)])],
            ),
        ] {
            let mut state = SpeakState::new();
            state.apply_event(enqueued_saying("a", false, "first"));
            for event in between {
                state.apply_event(event);
            }

            state.apply_event(enqueued_saying("a", false, "second"));

            let queue = state.queue_snapshot();
            assert_eq!(queue.len(), 1, "{case}");
            assert_eq!(queue[0].text, "second", "{case}");
        }
    }

    #[test]
    fn starting_a_reordered_item_leaves_the_remaining_rows_in_the_broadcast_sequence() {
        let mut state = seeded(&["a", "b", "c"]);
        state.apply_event(queue_changed(&[("b", false), ("c", false), ("a", false)]));

        state.apply_event(queue_changed(&[("c", false), ("a", false)]));
        state.apply_event(started("b"));

        assert_eq!(queue_ids(&state), ["c", "a"]);
        assert_eq!(
            state.now_speaking_snapshot().map(|ns| ns.request_id.0),
            Some("b".to_owned())
        );
    }

    #[test]
    fn clearing_the_pending_list_is_owned_by_the_empty_order_not_by_the_cleared_event() {
        let mut state = seeded(&["a", "b"]);
        state.apply_event(queue_changed(&[("b", false)]));
        state.apply_event(started("a"));

        state.apply_event(SpeakEvent::Cleared);
        assert_eq!(queue_ids(&state), ["b"]);
        assert!(state.now_speaking_snapshot().is_none());

        state.apply_event(queue_changed(&[]));
        assert!(queue_ids(&state).is_empty());
    }

    #[test]
    fn removed_neither_drops_the_row_nor_asks_for_a_repaint_on_its_own() {
        let mut state = seeded(&["a", "b"]);

        let changed = state.apply_event(SpeakEvent::Removed {
            request_id: rid("a"),
        });

        assert!(!changed);
        assert_eq!(queue_ids(&state), ["a", "b"]);
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
