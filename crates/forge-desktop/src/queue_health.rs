use std::collections::HashSet;

use forge_events::Event;
use forge_types::QueueId;

/// Topic-scoped observable entity for live queue health, fed by the runtime→UI bridge
/// (the sole owner of the bus→UI edge). It folds the `QueueScheduler`'s lifecycle
/// observability events into the set of currently-paused queues, keyed by [`QueueId`],
/// so the Queues console reflects pauses and resumes driven from anywhere (a
/// queue-control sub-action, another surface), not just its own buttons. The bridge
/// advances it and `cx.notify()`s, from which the observing [`crate::queues::QueuesView`]
/// repaints.
///
/// Only the paused set is bus-derivable today: the scheduler emits `queue.paused` /
/// `queue.resumed` carrying the queue id, matching the parity source's model (which
/// reads the same paused set off the scheduler). The per-queue pending / in-flight /
/// running counters are not attributed on the bus and stay at their empty state. The
/// queue roster itself (names, blocking, assigned-action counts) is read off storage and
/// carries the scheduler's own queue ids, so a pause/resume driven from anywhere lands on
/// the matching card.
pub struct QueueHealth {
    paused: HashSet<QueueId>,
}

impl QueueHealth {
    /// An empty, live-fed readout: no queue known-paused until the bridge folds the
    /// first `queue.paused` event.
    pub fn new() -> Self {
        Self {
            paused: HashSet::new(),
        }
    }

    /// Folds one queue-lifecycle observability event into the paused set: `queue.paused`
    /// marks a queue held, `queue.resumed` clears it. `queue.cleared` deliberately
    /// advances nothing — the scheduler carries pause state across the slot rebuild a
    /// clear performs, so a cleared queue keeps whatever paused state it already had.
    /// Reports whether the set actually changed so the bridge repaints only on a real
    /// change. Kept free of `cx` so it stays directly exercisable.
    pub fn apply_event(&mut self, event: &Event) -> bool {
        let Some(id) = queue_id_of(event) else {
            return false;
        };
        match event.kind.as_str() {
            "queue.paused" => self.paused.insert(id),
            "queue.resumed" => self.paused.remove(&id),
            _ => false,
        }
    }

    /// Whether the live scheduler currently reports this queue paused.
    pub fn is_paused(&self, id: QueueId) -> bool {
        self.paused.contains(&id)
    }
}

/// Extracts and parses the `queue_id` string field the queue-lifecycle events carry, or
/// `None` when the field is absent or not a well-formed id.
fn queue_id_of(event: &Event) -> Option<QueueId> {
    event
        .payload
        .get("queue_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
}
