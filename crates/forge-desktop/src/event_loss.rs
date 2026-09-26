use forge_components::tr;
use forge_runtime::delivery::{
    CHAT_HISTORY, EVENT_LOG, EVENT_TRAIL, RUN_HISTORY, TRIGGER_EVALUATOR, UNNAMED_OBSERVER,
    VIEWER_TRACKER,
};
use forge_runtime::{ConsumerLoss, DeliveryTier, LossCount};

use crate::chat_feed_bridge::UI_CHAT_FEED;

pub const UI_EVENTS: &str = "ui_events";
const UI_CHAT_DISPLAY: &str = "ui_chat_display";

/// Counts only ever grow: they cover this process's lifetime and reset on restart.
#[derive(Default)]
pub struct EventLoss {
    consumers: Vec<ConsumerLoss>,
    display_dropped: u64,
}

impl EventLoss {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_report(&mut self, report: Vec<ConsumerLoss>) -> bool {
        if self.consumers == report {
            return false;
        }
        self.consumers = report;
        true
    }

    pub fn record_display_dropped(&mut self, messages: u64) -> bool {
        if messages == 0 {
            return false;
        }
        self.display_dropped = self.display_dropped.saturating_add(messages);
        true
    }

    /// Consumers that lost anything; priority-lane losses first, then by total, largest first.
    pub fn rows(&self) -> Vec<ConsumerLoss> {
        let display = (self.display_dropped > 0).then(|| ConsumerLoss {
            consumer: UI_CHAT_DISPLAY,
            tier: DeliveryTier::Observer,
            loss: LossCount {
                skipped: self.display_dropped,
                ..LossCount::default()
            },
        });
        let mut rows: Vec<ConsumerLoss> = self
            .consumers
            .iter()
            .filter(|row| row.loss.total() > 0)
            .cloned()
            .chain(display)
            .collect();
        rows.sort_by(|a, b| {
            b.loss
                .priority_dropped
                .cmp(&a.loss.priority_dropped)
                .then(b.loss.total().cmp(&a.loss.total()))
                .then(a.consumer.cmp(b.consumer))
        });
        rows
    }

    pub fn total(&self) -> u64 {
        self.consumers
            .iter()
            .fold(self.display_dropped, |sum, row| {
                sum.saturating_add(row.loss.total())
            })
    }

    pub fn priority_dropped(&self) -> u64 {
        self.consumers.iter().fold(0, |sum: u64, row| {
            sum.saturating_add(row.loss.priority_dropped)
        })
    }
}

pub fn consumer_label(consumer: &str) -> String {
    match consumer {
        TRIGGER_EVALUATOR => tr!("event_loss_consumer_trigger_evaluator"),
        EVENT_LOG => tr!("event_loss_consumer_event_log"),
        CHAT_HISTORY => tr!("event_loss_consumer_chat_history"),
        VIEWER_TRACKER => tr!("event_loss_consumer_viewer_tracker"),
        RUN_HISTORY => tr!("event_loss_consumer_run_history"),
        EVENT_TRAIL => tr!("event_loss_consumer_event_trail"),
        UNNAMED_OBSERVER => tr!("event_loss_consumer_observer"),
        UI_EVENTS => tr!("event_loss_consumer_ui_events"),
        UI_CHAT_FEED => tr!("event_loss_consumer_ui_chat_feed"),
        UI_CHAT_DISPLAY => tr!("event_loss_consumer_ui_chat_display"),
        other => other.replace('_', " "),
    }
}

pub fn tier_label(tier: DeliveryTier) -> String {
    match tier {
        DeliveryTier::Critical => tr!("event_loss_tier_critical"),
        DeliveryTier::Observer => tr!("event_loss_tier_observer"),
    }
}

pub fn breakdown(loss: &LossCount) -> String {
    let count = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
    let mut parts = Vec::new();
    if loss.priority_dropped > 0 {
        parts.push(tr!(
            "event_loss_part_priority",
            count = count(loss.priority_dropped)
        ));
    }
    if loss.bulk_dropped > 0 {
        parts.push(tr!(
            "event_loss_part_bulk",
            count = count(loss.bulk_dropped)
        ));
    }
    if loss.skipped > 0 {
        parts.push(tr!("event_loss_part_skipped", count = count(loss.skipped)));
    }
    if loss.unwritten > 0 {
        parts.push(tr!(
            "event_loss_part_unwritten",
            count = count(loss.unwritten)
        ));
    }
    parts.join(" · ")
}
