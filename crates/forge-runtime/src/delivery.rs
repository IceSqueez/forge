use std::sync::Arc;

use arc_swap::ArcSwap;
use forge_events::{DeliveryLane, Event, EventSource};
use forge_registry::{TriggerRegistry, kind_matches_prefix};
use tokio::sync::mpsc::{self, error::TrySendError};

use crate::delivery_loss::ConsumerLossCounters;
use crate::event_ring::EventRing;

/// A lane counts as recovered only once this share of it is free again, so a consumer hovering
/// at the edge logs one episode instead of one per event.
const RECOVERY_HEADROOM_DIVISOR: usize = 2;

pub const TRIGGER_EVALUATOR: &str = "trigger_evaluator";
pub const EVENT_LOG: &str = "event_log";
pub const CHAT_HISTORY: &str = "chat_history";
pub const VIEWER_TRACKER: &str = "viewer_tracker";
pub const RUN_HISTORY: &str = "run_history";
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

const LINEAGE_WALK_CEILING: usize = 64;

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
        if is_run_telemetry(event) {
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

    /// Run telemetry whose causation chain starts at a declared flood-lane platform event stays
    /// out of durable storage; an ancestor already evicted from `ring` keeps the event durable.
    pub(crate) fn is_transient(&self, event: &Event, ring: &EventRing) -> bool {
        if !is_run_telemetry(event) {
            return false;
        }
        let mut cursor = event.caused_by;
        for _ in 0..LINEAGE_WALK_CEILING {
            let Some(parent) = cursor.and_then(|id| ring.get(id)) else {
                return false;
            };
            match parent.caused_by {
                Some(next) => cursor = Some(next),
                None => {
                    return parent.source != EventSource::Core
                        && self.lane_of(parent) == DeliveryLane::Bulk;
                }
            }
        }
        false
    }
}

fn is_run_telemetry(event: &Event) -> bool {
    event.source == EventSource::Core && CORE_BULK_KINDS.contains(&event.kind.as_str())
}

pub(crate) struct CriticalSink {
    priority: mpsc::Sender<Arc<Event>>,
    bulk: mpsc::Sender<Arc<Event>>,
    loss: Arc<ConsumerLossCounters>,
    durable_only: bool,
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
                durable_only: false,
            },
            CriticalSubscription {
                priority: priority_rx,
                bulk: bulk_rx,
            },
        )
    }

    /// Transient events never reach this sink; see `LaneTable::is_transient`.
    pub(crate) fn durable_only(mut self) -> Self {
        self.durable_only = true;
        self
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

    pub(crate) fn deliver(&self, event: &Arc<Event>, lane: DeliveryLane, transient: bool) {
        let sinks = self.0.load();
        let mut any_closed = false;
        for sink in sinks.iter() {
            if transient && sink.durable_only {
                any_closed |= sink.is_closed();
                continue;
            }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_registry::{
        EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
    };
    use forge_types::{EventId, TriggerConfig};
    use tracing::Level;

    use super::*;
    use crate::delivery_loss::{DeliveryTier, LossCount, LossLedger};
    use crate::test_support::log_capture::{CapturedLine, capture};

    const LANE_CAPACITY: usize = 4;
    const FLOOD: usize = LANE_CAPACITY * 5;
    const BULK_OVERFLOW: &str = "bulk events are being dropped";
    const PRIORITY_OVERFLOW: &str = "priority events are being dropped";
    const CAUGHT_UP: &str = "caught up";

    struct LaneDescriptor {
        source: Option<EventSource>,
        kind_prefix: &'static str,
        lane: DeliveryLane,
    }

    impl TriggerKindDescriptor for LaneDescriptor {
        fn id(&self) -> &str {
            self.kind_prefix
        }
        fn category(&self) -> TriggerCategory {
            TriggerCategory::Chat
        }
        fn label(&self) -> &str {
            ""
        }
        fn summary(&self) -> &str {
            ""
        }
        fn search_text(&self) -> &str {
            ""
        }
        fn icon_name(&self) -> &str {
            ""
        }
        fn platform_contract(&self) -> KindPlatformContract {
            KindPlatformContract::Universal
        }
        fn default_config(&self) -> TriggerConfig {
            TriggerConfig::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn condition_display(&self, _: &TriggerConfig) -> String {
            String::new()
        }
        fn event_filter(&self) -> EventFilter {
            EventFilter {
                source: self.source,
                kind_prefix: Some(self.kind_prefix.to_owned()),
            }
        }
        fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
            true
        }
        fn delivery_lane(&self) -> DeliveryLane {
            self.lane
        }
    }

    fn registry(declared: Vec<LaneDescriptor>) -> TriggerRegistry {
        let mut registry = TriggerRegistry::new();
        for descriptor in declared {
            registry.register(Box::new(descriptor)).unwrap();
        }
        registry
    }

    fn event(source: EventSource, kind: &str) -> Arc<Event> {
        Arc::new(Event::new(source, kind, serde_json::Value::Null))
    }

    fn core(kind: &str) -> Arc<Event> {
        event(EventSource::Core, kind)
    }

    fn sink(priority: usize, bulk: usize) -> (CriticalSink, CriticalSubscription, Arc<LossLedger>) {
        let ledger = LossLedger::new();
        let (sink, subscription) = CriticalSink::open(
            priority,
            bulk,
            ledger.counters("probe", DeliveryTier::Critical),
        );
        (sink, subscription, ledger)
    }

    fn loss(ledger: &LossLedger) -> LossCount {
        ledger.report()[0].loss
    }

    fn drain(subscription: &mut CriticalSubscription) -> Vec<EventId> {
        std::iter::from_fn(|| subscription.try_recv().map(|event| event.id)).collect()
    }

    fn mentioning<'a>(lines: &'a [CapturedLine], needle: &str) -> Vec<&'a CapturedLine> {
        lines.iter().filter(|line| line.mentions(needle)).collect()
    }

    #[test]
    fn lane_of_sheds_only_core_run_telemetry_and_declared_flood_kinds() {
        let table = LaneTable::from_registry(&registry(vec![
            LaneDescriptor {
                source: Some(EventSource::Twitch),
                kind_prefix: "twitch.channel.chat.message",
                lane: DeliveryLane::Bulk,
            },
            LaneDescriptor {
                source: None,
                kind_prefix: "custom.flood",
                lane: DeliveryLane::Bulk,
            },
            LaneDescriptor {
                source: Some(EventSource::Twitch),
                kind_prefix: "twitch.channel.cheer",
                lane: DeliveryLane::Priority,
            },
        ]));

        for (source, kind, expected) in [
            (
                EventSource::Twitch,
                "twitch.channel.chat.message",
                DeliveryLane::Bulk,
            ),
            (
                EventSource::Twitch,
                "twitch.channel.chat.message.extra",
                DeliveryLane::Bulk,
            ),
            (
                EventSource::Twitch,
                "twitch.channel.chat.message_delete",
                DeliveryLane::Priority,
            ),
            (
                EventSource::YouTube,
                "twitch.channel.chat.message",
                DeliveryLane::Priority,
            ),
            (EventSource::Server, "custom.flood", DeliveryLane::Bulk),
            (EventSource::Rhai, "custom.flood", DeliveryLane::Bulk),
            (
                EventSource::Twitch,
                "twitch.channel.cheer",
                DeliveryLane::Priority,
            ),
            (
                EventSource::Twitch,
                "twitch.never.declared",
                DeliveryLane::Priority,
            ),
            (EventSource::Core, "action.start", DeliveryLane::Bulk),
            (EventSource::Core, "subaction.run", DeliveryLane::Bulk),
            (EventSource::Core, "global.set", DeliveryLane::Priority),
            (EventSource::Twitch, "action.start", DeliveryLane::Priority),
        ] {
            assert_eq!(
                table.lane_of(&event(source, kind)),
                expected,
                "{source:?} {kind}"
            );
        }
    }

    #[test]
    fn declaring_lanes_replaces_the_previous_declaration() {
        let bus = crate::EventBus::new(Arc::new(crate::NullEventLogRepo));
        let chat = event(EventSource::Twitch, "twitch.channel.chat.message");
        let bulk_chat = || {
            registry(vec![LaneDescriptor {
                source: Some(EventSource::Twitch),
                kind_prefix: "twitch.channel.chat.message",
                lane: DeliveryLane::Bulk,
            }])
        };

        let undeclared = bus.lane_of(&chat);
        bus.declare_lanes(&bulk_chat());
        let declared = bus.lane_of(&chat);
        bus.declare_lanes(&registry(Vec::new()));
        let redeclared = bus.lane_of(&chat);

        assert_eq!(
            (undeclared, declared, redeclared),
            (
                DeliveryLane::Priority,
                DeliveryLane::Bulk,
                DeliveryLane::Priority
            )
        );
    }

    #[test]
    fn priority_events_are_all_kept_while_the_bulk_lane_floods() {
        let (sink, mut subscription, ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        for _ in 0..FLOOD {
            sink.offer(&core("action.start"), DeliveryLane::Bulk);
        }
        let priority: Vec<Arc<Event>> = (0..LANE_CAPACITY).map(|_| core("global.set")).collect();
        for event in &priority {
            sink.offer(event, DeliveryLane::Priority);
        }

        let drained = drain(&mut subscription);

        assert_eq!(
            (&drained[..LANE_CAPACITY], loss(&ledger).priority_dropped),
            (
                &priority.iter().map(|event| event.id).collect::<Vec<_>>()[..],
                0
            )
        );
    }

    #[test]
    fn a_bulk_flood_counts_exactly_the_events_beyond_the_lane_capacity() {
        let (sink, _subscription, ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        for _ in 0..FLOOD {
            sink.offer(&core("action.start"), DeliveryLane::Bulk);
        }

        assert_eq!(
            loss(&ledger),
            LossCount {
                priority_dropped: 0,
                bulk_dropped: (FLOOD - LANE_CAPACITY) as u64,
                skipped: 0,
                unwritten: 0,
            }
        );
    }

    #[test]
    fn a_full_lane_keeps_its_oldest_events_and_sheds_the_newest() {
        let (sink, mut subscription, ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let offered: Vec<Arc<Event>> = (0..LANE_CAPACITY + 2).map(|_| core("global.set")).collect();
        for event in &offered {
            sink.offer(event, DeliveryLane::Priority);
        }

        let kept = drain(&mut subscription);

        assert_eq!(
            (kept, loss(&ledger).priority_dropped),
            (
                offered[..LANE_CAPACITY]
                    .iter()
                    .map(|event| event.id)
                    .collect(),
                2
            )
        );
    }

    #[tokio::test]
    async fn recv_hands_out_queued_priority_events_before_earlier_bulk_ones() {
        let (sink, mut subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let bulk = [core("action.start"), core("action.done")];
        let priority = [core("global.set"), core("timer.tick")];
        for event in &bulk {
            sink.offer(event, DeliveryLane::Bulk);
        }
        for event in &priority {
            sink.offer(event, DeliveryLane::Priority);
        }

        let mut order = Vec::new();
        for _ in 0..4 {
            order.push(subscription.recv().await.unwrap().id);
        }

        assert_eq!(
            order,
            vec![priority[0].id, priority[1].id, bulk[0].id, bulk[1].id]
        );
    }

    #[test]
    fn try_recv_drains_priority_first_and_then_the_bulk_backlog() {
        let (sink, mut subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let bulk = core("action.start");
        let priority = core("global.set");
        sink.offer(&bulk, DeliveryLane::Bulk);
        sink.offer(&priority, DeliveryLane::Priority);

        assert_eq!(drain(&mut subscription), vec![priority.id, bulk.id]);
    }

    #[tokio::test]
    async fn recv_delivers_the_bulk_backlog_before_reporting_the_bus_gone() {
        let (sink, mut subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let queued = core("action.start");
        sink.offer(&queued, DeliveryLane::Bulk);
        drop(sink);

        let first = subscription.recv().await.map(|event| event.id);
        let second = subscription.recv().await.map(|event| event.id);

        assert_eq!((first, second), (Some(queued.id), None));
    }

    #[test]
    fn delivery_forgets_a_consumer_that_went_away() {
        let ledger = LossLedger::new();
        let sinks = CriticalSinks::new();
        let open = |name| {
            CriticalSink::open(
                LANE_CAPACITY,
                LANE_CAPACITY,
                ledger.counters(name, DeliveryTier::Critical),
            )
        };
        let (gone, gone_subscription) = open("gone");
        let (kept, _kept_subscription) = open("kept");
        sinks.add(gone);
        sinks.add(kept);
        drop(gone_subscription);

        sinks.deliver(&core("global.set"), DeliveryLane::Priority, false);

        assert_eq!(sinks.0.load().len(), 1);
    }

    #[test]
    fn a_lane_flapping_at_its_edge_logs_one_overflow_episode() {
        let (sink, mut subscription, ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let lines = capture(Level::WARN, || {
            for _ in 0..=LANE_CAPACITY {
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
            }
            for _ in 0..FLOOD {
                subscription.try_recv();
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
            }
        });

        assert_eq!(
            (
                mentioning(&lines, BULK_OVERFLOW).len(),
                mentioning(&lines, CAUGHT_UP).len(),
                loss(&ledger).bulk_dropped,
            ),
            (1, 0, (FLOOD + 1) as u64),
            "a consumer hovering at a full lane must not log once per dropped event"
        );
    }

    #[test]
    fn a_lane_counts_as_recovered_only_once_half_of_it_is_free_again() {
        for (drained, recovered) in [(2, 0), (3, 1)] {
            let (sink, mut subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
            let lines = capture(Level::WARN, || {
                for _ in 0..=LANE_CAPACITY {
                    sink.offer(&core("action.start"), DeliveryLane::Bulk);
                }
                for _ in 0..drained {
                    subscription.try_recv();
                }
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
            });

            assert_eq!(
                mentioning(&lines, CAUGHT_UP).len(),
                recovered,
                "after draining {drained} of {LANE_CAPACITY}"
            );
        }
    }

    #[test]
    fn an_overflow_after_recovery_opens_a_new_logged_episode() {
        let (sink, mut subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let lines = capture(Level::WARN, || {
            for _ in 0..=LANE_CAPACITY {
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
            }
            drain(&mut subscription);
            for _ in 0..=LANE_CAPACITY {
                sink.offer(&core("action.start"), DeliveryLane::Bulk);
            }
        });

        assert_eq!(mentioning(&lines, BULK_OVERFLOW).len(), 2);
    }

    #[test]
    fn only_a_priority_overflow_is_logged_at_error_level() {
        let (sink, _subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        let lines = capture(Level::WARN, || {
            for lane in [DeliveryLane::Priority, DeliveryLane::Bulk] {
                for _ in 0..=LANE_CAPACITY {
                    sink.offer(&core("action.start"), lane);
                }
            }
        });

        let levels = |needle| {
            mentioning(&lines, needle)
                .into_iter()
                .map(|line| line.level)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            (levels(PRIORITY_OVERFLOW), levels(BULK_OVERFLOW)),
            (vec![Level::ERROR], vec![Level::WARN])
        );
    }

    fn chat_bulk_table() -> LaneTable {
        LaneTable::from_registry(&registry(vec![LaneDescriptor {
            source: Some(EventSource::Twitch),
            kind_prefix: "twitch.channel.chat.message",
            lane: DeliveryLane::Bulk,
        }]))
    }

    fn chat() -> Arc<Event> {
        event(EventSource::Twitch, "twitch.channel.chat.message")
    }

    fn follow() -> Arc<Event> {
        event(EventSource::Twitch, "twitch.channel.follow")
    }

    fn child(parent: &Event, kind: &str) -> Arc<Event> {
        Arc::new(Event::caused_by(
            EventSource::Core,
            kind,
            serde_json::Value::Null,
            parent.id,
        ))
    }

    /// Every event from the root down to the last one, the last being the event under test.
    fn lineage(root: Arc<Event>, kinds: &[&str]) -> Vec<Arc<Event>> {
        let mut chain = vec![root];
        for kind in kinds {
            let next = child(chain.last().unwrap(), kind);
            chain.push(next);
        }
        chain
    }

    fn transient_in_ring(table: &LaneTable, chain: &[Arc<Event>], retained_from: usize) -> bool {
        let mut ring = EventRing::new(chain.len());
        for ancestor in &chain[retained_from..chain.len() - 1] {
            ring.push(Arc::clone(ancestor));
        }
        table.is_transient(chain.last().unwrap(), &ring)
    }

    #[test]
    fn only_run_telemetry_rooted_at_a_declared_flood_platform_event_is_transient() {
        let table = chat_bulk_table();
        let uncaused_run = core("action.start");
        for (case, chain, expected) in [
            (
                "run of a chat message",
                lineage(chat(), &["action.start"]),
                true,
            ),
            (
                "nested run of a chat message",
                lineage(
                    chat(),
                    &[
                        "action.start",
                        "subaction.run",
                        "action.start",
                        "action.done",
                    ],
                ),
                true,
            ),
            (
                "run of a follow",
                lineage(follow(), &["action.start"]),
                false,
            ),
            (
                "step of a manual run",
                lineage(uncaused_run, &["subaction.run"]),
                false,
            ),
            (
                "run of a timer tick",
                lineage(core("timer.tick"), &["action.start"]),
                false,
            ),
            (
                "global written by a chat run",
                lineage(chat(), &["action.start", "global.set"]),
                false,
            ),
            ("uncaused run", vec![core("action.start")], false),
        ] {
            assert_eq!(transient_in_ring(&table, &chain, 0), expected, "{case}");
        }
    }

    #[test]
    fn run_telemetry_whose_root_left_the_ring_stays_durable() {
        let chain = lineage(chat(), &["action.start", "subaction.run"]);

        assert!(!transient_in_ring(&chat_bulk_table(), &chain, 1));
    }

    #[test]
    fn the_lineage_walk_gives_up_one_hop_past_its_ceiling() {
        let table = chat_bulk_table();
        let verdict = |hops: usize| {
            let chain = lineage(chat(), &vec!["action.start"; hops]);
            transient_in_ring(&table, &chain, 0)
        };

        assert_eq!(
            (
                verdict(LINEAGE_WALK_CEILING),
                verdict(LINEAGE_WALK_CEILING + 1)
            ),
            (true, false)
        );
    }

    #[test]
    fn a_durable_only_sink_skips_transient_events_that_a_regular_sink_still_receives() {
        let sinks = CriticalSinks::new();
        let (durable, mut durable_subscription, _durable_ledger) =
            sink(LANE_CAPACITY, LANE_CAPACITY);
        let (regular, mut regular_subscription, _regular_ledger) =
            sink(LANE_CAPACITY, LANE_CAPACITY);
        sinks.add(durable.durable_only());
        sinks.add(regular);
        let transient = core("action.start");
        let durable_event = core("action.start");

        sinks.deliver(&transient, DeliveryLane::Bulk, true);
        sinks.deliver(&durable_event, DeliveryLane::Bulk, false);

        assert_eq!(
            (
                drain(&mut durable_subscription),
                drain(&mut regular_subscription)
            ),
            (vec![durable_event.id], vec![transient.id, durable_event.id])
        );
    }

    #[test]
    fn a_closed_durable_only_sink_is_forgotten_on_a_transient_delivery() {
        let sinks = CriticalSinks::new();
        let (durable, durable_subscription, _ledger) = sink(LANE_CAPACITY, LANE_CAPACITY);
        sinks.add(durable.durable_only());
        drop(durable_subscription);

        sinks.deliver(&core("action.start"), DeliveryLane::Bulk, true);

        assert_eq!(sinks.0.load().len(), 0);
    }

    struct StoredIds(std::sync::Mutex<Vec<EventId>>);

    #[async_trait::async_trait]
    impl forge_storage::EventLogRepo for StoredIds {
        async fn insert(&self, event: &Event) -> Result<(), forge_storage::StorageError> {
            self.0.lock().unwrap().push(event.id);
            Ok(())
        }
        async fn get(&self, _: EventId) -> Result<Option<Event>, forge_storage::StorageError> {
            Ok(None)
        }
        async fn recent(&self, _: usize) -> Result<Vec<Event>, forge_storage::StorageError> {
            Ok(Vec::new())
        }
        async fn recent_since(
            &self,
            _: usize,
            _: Option<EventId>,
        ) -> Result<Vec<Event>, forge_storage::StorageError> {
            Ok(Vec::new())
        }
        async fn prune_before(
            &self,
            _: time::OffsetDateTime,
        ) -> Result<u64, forge_storage::StorageError> {
            Ok(0)
        }
    }

    /// Publishes a chat message, a follow, a manual run and one run under each; returns the ids
    /// in publish order.
    fn publish_mixed_traffic(bus: &crate::EventBus) -> Vec<EventId> {
        let chat = chat();
        let follow = follow();
        let manual = core("action.start");
        let events = [
            Arc::clone(&chat),
            child(&chat, "action.start"),
            Arc::clone(&follow),
            child(&follow, "action.start"),
            Arc::clone(&manual),
            child(&manual, "subaction.run"),
        ];
        let ids = events.iter().map(|event| event.id).collect();
        for event in events {
            bus.publish(Arc::unwrap_or_clone(event));
        }
        ids
    }

    fn mixed_bus() -> (Arc<crate::EventBus>, Arc<StoredIds>) {
        let repo = Arc::new(StoredIds(std::sync::Mutex::new(Vec::new())));
        let bus = crate::EventBus::new(Arc::clone(&repo) as Arc<dyn forge_storage::EventLogRepo>);
        bus.declare_lanes(&registry(vec![LaneDescriptor {
            source: Some(EventSource::Twitch),
            kind_prefix: "twitch.channel.chat.message",
            lane: DeliveryLane::Bulk,
        }]));
        (bus, repo)
    }

    #[tokio::test]
    async fn the_event_log_keeps_everything_but_the_run_telemetry_of_chat_messages() {
        let (bus, repo) = mixed_bus();
        crate::EventBus::spawn_flush_task(Arc::clone(&bus));

        let ids = publish_mixed_traffic(&bus);
        bus.shutdown();
        bus.await_flush().await;

        let mut stored = repo.0.lock().unwrap().clone();
        stored.sort();
        let mut expected = vec![ids[0], ids[2], ids[3], ids[4], ids[5]];
        expected.sort();
        assert_eq!(
            stored, expected,
            "the chat message, the follow and its run, the manual run and its step"
        );
    }

    #[tokio::test]
    async fn an_observer_sees_chat_run_telemetry_the_event_log_skips() {
        let (bus, _repo) = mixed_bus();
        crate::EventBus::spawn_flush_task(Arc::clone(&bus));
        let mut feed = bus.subscribe_observer("feed");

        let ids = publish_mixed_traffic(&bus);

        let seen: Vec<EventId> =
            std::iter::from_fn(|| feed.try_recv().unwrap().map(|event| event.id)).collect();
        assert_eq!(seen, ids);
    }
}
