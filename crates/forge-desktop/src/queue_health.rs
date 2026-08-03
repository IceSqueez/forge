use std::collections::HashMap;

use forge_events::Event;
use forge_runtime::{QueueIntake, QueueMode, QueueProcessing};
use forge_types::QueueId;

pub struct QueueHealth {
    modes: HashMap<QueueId, QueueMode>,
}

impl QueueHealth {
    pub fn new() -> Self {
        Self {
            modes: HashMap::new(),
        }
    }

    pub fn apply_event(&mut self, event: &Event) -> bool {
        let Some(id) = queue_id_of(event) else {
            return false;
        };
        let Some(mode) = mode_of(event) else {
            return false;
        };
        self.modes.insert(id, mode) != Some(mode)
    }

    pub fn mode(&self, id: QueueId) -> Option<QueueMode> {
        self.modes.get(&id).copied()
    }
}

fn queue_id_of(event: &Event) -> Option<QueueId> {
    event
        .payload
        .get("queue_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
}

fn mode_of(event: &Event) -> Option<QueueMode> {
    let processing = match event.payload.get("processing")?.as_str()? {
        "running" => QueueProcessing::Running,
        "frozen" => QueueProcessing::Frozen,
        _ => return None,
    };
    let intake = match event.payload.get("intake")?.as_str()? {
        "accept" => QueueIntake::Accept,
        "skip" => QueueIntake::Skip,
        _ => return None,
    };
    Some(QueueMode { processing, intake })
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
        let no_queue_id = Event::new(
            EventSource::Core,
            "queue.held",
            json!({ "processing": "frozen", "intake": "accept" }),
        );
        let unparseable_id = Event::new(
            EventSource::Core,
            "queue.held",
            json!({ "queue_id": "not-a-queue-id", "processing": "frozen", "intake": "accept" }),
        );

        for event in [
            cleared,
            no_queue_id,
            unparseable_id,
            mode_event("queue.held", id, "thawing", "accept"),
            mode_event("queue.held", id, "frozen", "hoard"),
        ] {
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
}
