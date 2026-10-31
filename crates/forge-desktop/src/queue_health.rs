use std::collections::HashMap;

use forge_events::Event;
use forge_runtime::{
    QueueDepth, QueueDepths, QueueMode, QueueModeChanged, QueueRunning, RunningAction,
};
use forge_types::QueueId;

pub(crate) struct QueueHealth {
    modes: HashMap<QueueId, QueueMode>,
    depths: QueueDepths,
    running: QueueRunning,
}

impl QueueHealth {
    pub(crate) fn new() -> Self {
        Self {
            modes: HashMap::new(),
            depths: QueueDepths::new(),
            running: QueueRunning::new(),
        }
    }

    pub(crate) fn apply_depths(&mut self, depths: QueueDepths) -> bool {
        if self.depths == depths {
            return false;
        }
        self.modes.retain(|id, _| depths.contains_key(id));
        self.depths = depths;
        true
    }

    pub(crate) fn depth(&self, id: QueueId) -> Option<QueueDepth> {
        self.depths.get(&id).copied()
    }

    pub(crate) fn apply_running(&mut self, running: QueueRunning) -> bool {
        if self.running == running {
            return false;
        }
        self.running = running;
        true
    }

    pub(crate) fn running(&self, id: QueueId) -> &[RunningAction] {
        self.running.get(&id).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn apply_event(&mut self, event: &Event) -> bool {
        let Some(changed) = QueueModeChanged::from_event(event) else {
            return false;
        };
        self.modes.insert(changed.queue_id, changed.mode) != Some(changed.mode)
    }

    pub(crate) fn mode(&self, id: QueueId) -> Option<QueueMode> {
        self.modes.get(&id).copied()
    }
}

#[cfg(test)]
mod tests {
    use forge_events::EventSource;
    use serde_json::json;

    use super::*;

    fn mode_event(kind: &str, id: QueueId, processing: &str, intake: &str) -> Event {
        Event::new(
            EventSource::Core,
            kind,
            json!({
                "queue_id": id.to_string(),
                "queue_name": "default",
                "processing": processing,
                "intake": intake,
            }),
        )
    }

    #[test]
    fn each_mode_change_payload_maps_onto_the_queue_mode() {
        for (kind, processing, intake, expected) in [
            ("queue.resumed", "running", "accept", QueueMode::RUNNING),
            ("queue.draining", "running", "skip", QueueMode::DRAINING),
            ("queue.held", "frozen", "accept", QueueMode::HOLDING),
            ("queue.paused", "frozen", "skip", QueueMode::PAUSED),
        ] {
            let id = QueueId::new();
            let mut health = QueueHealth::new();
            assert!(
                health.apply_event(&mode_event(kind, id, processing, intake)),
                "{kind} must register as a change"
            );
            assert_eq!(
                health.mode(id),
                Some(expected),
                "{kind} maps to the wrong mode"
            );
        }
    }

    #[test]
    fn only_an_actually_different_mode_reports_a_change() {
        let id = QueueId::new();
        let mut health = QueueHealth::new();
        let paused = mode_event("queue.paused", id, "frozen", "skip");

        assert!(
            health.apply_event(&paused),
            "the first mode seen for a queue is a change"
        );
        assert!(
            !health.apply_event(&paused),
            "a repeated mode must not request a repaint"
        );
        assert!(
            health.apply_event(&mode_event("queue.draining", id, "running", "skip")),
            "flipping one axis must report a change"
        );
    }

    #[test]
    fn events_without_a_readable_mode_leave_the_tracked_mode_untouched() {
        let id = QueueId::new();
        let mut health = QueueHealth::new();
        health.apply_event(&mode_event("queue.paused", id, "frozen", "skip"));

        let cleared = Event::new(
            EventSource::Core,
            "queue.cleared",
            json!({ "queue_id": id.to_string(), "queue_name": "default", "keep_current": true }),
        );
        for event in [cleared, mode_event("queue.held", id, "thawing", "accept")] {
            assert!(
                !health.apply_event(&event),
                "unreadable {} must not report a change",
                event.kind
            );
        }

        assert_eq!(
            health.mode(id),
            Some(QueueMode::PAUSED),
            "malformed events must not overwrite the tracked mode"
        );
    }

    #[test]
    fn a_depth_snapshot_reports_a_change_only_when_it_differs_from_the_held_one() {
        let id = QueueId::new();
        let busy = QueueDepths::from([(
            id,
            QueueDepth {
                pending: 2,
                in_flight: 1,
                overflowed: 0,
            },
        )]);
        let mut health = QueueHealth::new();

        assert_eq!(
            (
                health.apply_depths(busy.clone()),
                health.apply_depths(busy),
                health.apply_depths(QueueDepths::new()),
                health.depth(id),
            ),
            (true, false, true, None),
            "new depths repaint, a repeat does not, a removed queue does and is forgotten"
        );
    }

    #[test]
    fn a_queue_missing_from_a_new_depth_snapshot_loses_its_tracked_mode() {
        let (kept, removed) = (QueueId::new(), QueueId::new());
        let idle = QueueDepth {
            pending: 0,
            in_flight: 0,
            overflowed: 0,
        };
        let mut health = QueueHealth::new();
        health.apply_depths(QueueDepths::from([(kept, idle), (removed, idle)]));
        health.apply_event(&mode_event("queue.paused", kept, "frozen", "skip"));
        health.apply_event(&mode_event("queue.held", removed, "frozen", "accept"));

        health.apply_depths(QueueDepths::from([(kept, idle)]));

        assert_eq!(
            (health.mode(kept), health.mode(removed)),
            (Some(QueueMode::PAUSED), None)
        );
    }

    fn run() -> RunningAction {
        RunningAction {
            action_id: forge_types::ActionId::new(),
            started_at: std::time::Instant::now(),
        }
    }

    #[test]
    fn only_a_different_running_list_reports_a_change() {
        let id = QueueId::new();
        let mut health = QueueHealth::new();
        let busy = QueueRunning::from([(id, vec![run()])]);

        assert!(
            health.apply_running(busy.clone()),
            "the first run is a change"
        );
        assert!(
            !health.apply_running(busy),
            "a repeated list must not request a repaint"
        );
        assert!(
            health.apply_running(QueueRunning::new()),
            "the run ending is a change"
        );
    }

    #[test]
    fn running_lists_each_queues_own_runs_and_nothing_for_an_unknown_queue() {
        let (id, other) = (QueueId::new(), QueueId::new());
        let started = run();
        let mut health = QueueHealth::new();

        health.apply_running(QueueRunning::from([(id, vec![started])]));

        assert_eq!(health.running(id), [started]);
        assert!(health.running(other).is_empty());
    }
}
