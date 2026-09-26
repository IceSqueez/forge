use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use forge_events::DeliveryLane;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DeliveryTier {
    Critical,
    Observer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LossCount {
    pub priority_dropped: u64,
    pub bulk_dropped: u64,
    /// Observer lag: the broadcast reports how many events were skipped, not which lane they rode.
    pub skipped: u64,
}

impl LossCount {
    pub fn total(&self) -> u64 {
        self.priority_dropped + self.bulk_dropped + self.skipped
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerLoss {
    pub consumer: &'static str,
    pub tier: DeliveryTier,
    pub loss: LossCount,
}

#[derive(Default)]
struct LaneLoss {
    dropped: AtomicU64,
    overflowing: AtomicBool,
}

pub(crate) struct ConsumerLossCounters {
    consumer: &'static str,
    tier: DeliveryTier,
    priority: LaneLoss,
    bulk: LaneLoss,
    skipped: AtomicU64,
    changed: Arc<watch::Sender<u64>>,
}

impl ConsumerLossCounters {
    pub(crate) fn lane_dropped(&self, lane: DeliveryLane) {
        let counter = self.lane(lane);
        let dropped = counter.dropped.fetch_add(1, Ordering::Relaxed) + 1;
        if !counter.overflowing.swap(true, Ordering::Relaxed) {
            match lane {
                DeliveryLane::Priority => tracing::error!(
                    consumer = self.consumer,
                    dropped,
                    "a lossless consumer's priority lane is full; priority events are being dropped"
                ),
                DeliveryLane::Bulk => tracing::warn!(
                    consumer = self.consumer,
                    dropped,
                    "a lossless consumer fell behind; bulk events are being dropped"
                ),
            }
        }
        self.notify();
    }

    pub(crate) fn is_overflowing(&self, lane: DeliveryLane) -> bool {
        self.lane(lane).overflowing.load(Ordering::Relaxed)
    }

    pub(crate) fn lane_recovered(&self, lane: DeliveryLane) {
        let counter = self.lane(lane);
        if counter.overflowing.swap(false, Ordering::Relaxed) {
            tracing::warn!(
                consumer = self.consumer,
                lane = ?lane,
                dropped = counter.dropped.load(Ordering::Relaxed),
                "a lossless consumer caught up; its lane accepts events again"
            );
        }
    }

    pub(crate) fn lagged(&self, skipped: u64) {
        self.skipped.fetch_add(skipped, Ordering::Relaxed);
        self.notify();
    }

    fn lane(&self, lane: DeliveryLane) -> &LaneLoss {
        match lane {
            DeliveryLane::Priority => &self.priority,
            DeliveryLane::Bulk => &self.bulk,
        }
    }

    fn notify(&self) {
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    fn snapshot(&self) -> ConsumerLoss {
        ConsumerLoss {
            consumer: self.consumer,
            tier: self.tier,
            loss: LossCount {
                priority_dropped: self.priority.dropped.load(Ordering::Relaxed),
                bulk_dropped: self.bulk.dropped.load(Ordering::Relaxed),
                skipped: self.skipped.load(Ordering::Relaxed),
            },
        }
    }
}

/// Keyed by consumer name and tier, so re-subscribing under the same name never adds an entry.
pub(crate) struct LossLedger {
    consumers: Mutex<BTreeMap<(&'static str, DeliveryTier), Arc<ConsumerLossCounters>>>,
    changed: Arc<watch::Sender<u64>>,
}

impl LossLedger {
    pub(crate) fn new() -> Arc<Self> {
        let (changed, _) = watch::channel(0);
        Arc::new(Self {
            consumers: Mutex::new(BTreeMap::new()),
            changed: Arc::new(changed),
        })
    }

    pub(crate) fn counters(
        &self,
        consumer: &'static str,
        tier: DeliveryTier,
    ) -> Arc<ConsumerLossCounters> {
        let mut consumers = self.consumers.lock().unwrap_or_else(|p| p.into_inner());
        Arc::clone(consumers.entry((consumer, tier)).or_insert_with(|| {
            Arc::new(ConsumerLossCounters {
                consumer,
                tier,
                priority: LaneLoss::default(),
                bulk: LaneLoss::default(),
                skipped: AtomicU64::new(0),
                changed: Arc::clone(&self.changed),
            })
        }))
    }

    pub(crate) fn report(&self) -> Vec<ConsumerLoss> {
        self.consumers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|counters| counters.snapshot())
            .collect()
    }

    pub(crate) fn watch(self: &Arc<Self>) -> LossWatch {
        LossWatch {
            changed: self.changed.subscribe(),
            ledger: Arc::downgrade(self),
        }
    }
}

/// Coalescing: a burst of drops wakes the watcher once with the latest totals.
pub struct LossWatch {
    changed: watch::Receiver<u64>,
    ledger: Weak<LossLedger>,
}

impl LossWatch {
    pub fn current(&self) -> Vec<ConsumerLoss> {
        self.ledger
            .upgrade()
            .map(|ledger| ledger.report())
            .unwrap_or_default()
    }

    /// `None` once the bus behind this watch is gone.
    pub async fn changed(&mut self) -> Option<Vec<ConsumerLoss>> {
        self.changed.changed().await.ok()?;
        self.changed.borrow_and_update();
        Some(self.ledger.upgrade()?.report())
    }
}
