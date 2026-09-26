use std::sync::Arc;

use arc_swap::ArcSwap;
use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{TriggerRegistry, kind_matches_prefix};
use tokio::sync::mpsc::{self, error::TrySendError};

use crate::delivery_loss::ConsumerLossCounters;

/// A lane counts as recovered only once this share of it is free again, so a consumer hovering
/// at the edge logs one episode instead of one per event.
const RECOVERY_HEADROOM_DIVISOR: usize = 2;

pub const TRIGGER_EVALUATOR: &str = "trigger_evaluator";
pub const EVENT_LOG: &str = "event_log";
pub const CHAT_HISTORY: &str = "chat_history";
pub const CHAT_MODERATION: &str = "chat_moderation";
pub const VIEWER_TRACKER: &str = "viewer_tracker";
pub const EVENT_TRAIL: &str = "event_trail";
pub const UNNAMED_OBSERVER: &str = "observer";

/// Per-execution telemetry the runtime emits several times for every triggered action.
const CORE_BULK_KINDS: &[&str] = &[
    "action.start",
    "action.done",
    "action.skipped",
    "subaction.run",
    "subaction.done",
    "command.matched",
    "trigger.blocked",
];

struct BulkRule {
    source: Option<EventSource>,
    kind_prefix: Option<String>,
}

#[derive(Default)]
pub(crate) struct LaneTable {
    declared: Vec<BulkRule>,
}

impl LaneTable {
    pub(crate) fn from_registry(registry: &TriggerRegistry) -> Self {
        let declared = registry
            .all()
            .filter(|descriptor| descriptor.delivery_lane() == DeliveryLane::Bulk)
            .map(|descriptor| {
                let filter = descriptor.event_filter();
                BulkRule {
                    source: filter.source,
                    kind_prefix: filter.kind_prefix,
                }
            })
            .collect();
        Self { declared }
    }

    pub(crate) fn lane_of(&self, event: &Event) -> DeliveryLane {
        if event.source == EventSource::Core && CORE_BULK_KINDS.contains(&event.kind.as_str()) {
            return DeliveryLane::Bulk;
        }
        let declared_bulk = self.declared.iter().any(|rule| {
            rule.source.is_none_or(|source| source == event.source)
                && rule
                    .kind_prefix
                    .as_deref()
                    .is_none_or(|prefix| kind_matches_prefix(&event.kind, prefix))
        });
        if declared_bulk {
            DeliveryLane::Bulk
        } else {
            DeliveryLane::Priority
        }
    }
}

pub(crate) struct CriticalSink {
    priority: mpsc::Sender<Arc<Event>>,
    bulk: mpsc::Sender<Arc<Event>>,
    loss: Arc<ConsumerLossCounters>,
}

impl CriticalSink {
    pub(crate) fn open(
        priority_capacity: usize,
        bulk_capacity: usize,
        loss: Arc<ConsumerLossCounters>,
    ) -> (Self, CriticalSubscription) {
        let (priority_tx, priority_rx) = mpsc::channel(priority_capacity.max(1));
        let (bulk_tx, bulk_rx) = mpsc::channel(bulk_capacity.max(1));
        (
            Self {
                priority: priority_tx,
                bulk: bulk_tx,
                loss,
            },
            CriticalSubscription {
                priority: priority_rx,
                bulk: bulk_rx,
            },
        )
    }

    /// `false` once the consumer is gone, so the bus can forget this sink.
    pub(crate) fn offer(&self, event: &Arc<Event>, lane: DeliveryLane) -> bool {
        let sender = match lane {
            DeliveryLane::Priority => &self.priority,
            DeliveryLane::Bulk => &self.bulk,
        };
        match sender.try_send(Arc::clone(event)) {
            Ok(()) => {
                if self.loss.is_overflowing(lane)
                    && sender.capacity() >= sender.max_capacity() / RECOVERY_HEADROOM_DIVISOR
                {
                    self.loss.lane_recovered(lane);
                }
                true
            }
            Err(TrySendError::Full(_)) => {
                self.loss.lane_dropped(lane);
                true
            }
            Err(TrySendError::Closed(_)) => false,
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.priority.is_closed()
    }
}

pub(crate) struct CriticalSinks(ArcSwap<Vec<Arc<CriticalSink>>>);

impl CriticalSinks {
    pub(crate) fn new() -> Self {
        Self(ArcSwap::from_pointee(Vec::new()))
    }

    pub(crate) fn add(&self, sink: CriticalSink) {
        let sink = Arc::new(sink);
        self.0.rcu(|sinks| {
            let mut next: Vec<Arc<CriticalSink>> = sinks
                .iter()
                .filter(|existing| !existing.is_closed())
                .cloned()
                .collect();
            next.push(Arc::clone(&sink));
            next
        });
    }

    pub(crate) fn deliver(&self, event: &Arc<Event>, lane: DeliveryLane) {
        let sinks = self.0.load();
        let mut any_closed = false;
        for sink in sinks.iter() {
            any_closed |= !sink.offer(event, lane);
        }
        if any_closed {
            self.0.rcu(|sinks| {
                sinks
                    .iter()
                    .filter(|sink| !sink.is_closed())
                    .cloned()
                    .collect::<Vec<_>>()
            });
        }
    }
}

/// Lossless per-consumer delivery: priority events are handed out before any queued bulk event.
pub struct CriticalSubscription {
    priority: mpsc::Receiver<Arc<Event>>,
    bulk: mpsc::Receiver<Arc<Event>>,
}

impl CriticalSubscription {
    /// Cancel-safe; `None` once the bus is gone and both lanes are drained.
    pub async fn recv(&mut self) -> Option<Arc<Event>> {
        tokio::select! {
            biased;
            Some(event) = self.priority.recv() => Some(event),
            Some(event) = self.bulk.recv() => Some(event),
            else => None,
        }
    }

    /// `None` when both lanes are momentarily empty.
    pub fn try_recv(&mut self) -> Option<Arc<Event>> {
        self.priority
            .try_recv()
            .ok()
            .or_else(|| self.bulk.try_recv().ok())
    }
}

