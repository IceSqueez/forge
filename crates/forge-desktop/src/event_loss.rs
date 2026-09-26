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

#[cfg(test)]
mod tests {
    use forge_runtime::delivery::{EVENT_LOG, RUN_HISTORY, TRIGGER_EVALUATOR, VIEWER_TRACKER};
    use forge_runtime::{ConsumerLoss, DeliveryTier, LossCount};

    use super::{EventLoss, UI_CHAT_DISPLAY, UI_EVENTS, breakdown, consumer_label};

    fn lost(consumer: &'static str, tier: DeliveryTier, loss: LossCount) -> ConsumerLoss {
        ConsumerLoss {
            consumer,
            tier,
            loss,
        }
    }

    fn priority(n: u64) -> LossCount {
        LossCount {
            priority_dropped: n,
            ..LossCount::default()
        }
    }

    fn skipped(n: u64) -> LossCount {
        LossCount {
            skipped: n,
            ..LossCount::default()
        }
    }

    fn row_names(loss: &EventLoss) -> Vec<&'static str> {
        loss.rows().iter().map(|row| row.consumer).collect()
    }

    #[test]
    fn consumers_that_lost_nothing_leave_the_no_events_lost_state() {
        let mut loss = EventLoss::new();
        loss.apply_report(vec![
            lost(EVENT_LOG, DeliveryTier::Critical, LossCount::default()),
            lost(UI_EVENTS, DeliveryTier::Observer, LossCount::default()),
        ]);

        assert_eq!(
            (loss.rows().len(), loss.total(), loss.priority_dropped()),
            (0, 0, 0)
        );
    }

    #[test]
    fn totals_sum_every_consumer_and_the_chat_display_but_priority_counts_only_the_lanes() {
        let mut loss = EventLoss::new();
        loss.apply_report(vec![
            lost(
                TRIGGER_EVALUATOR,
                DeliveryTier::Critical,
                LossCount {
                    priority_dropped: 2,
                    bulk_dropped: 3,
                    skipped: 0,
                    unwritten: 1,
                },
            ),
            lost(EVENT_LOG, DeliveryTier::Critical, priority(5)),
            lost(UI_EVENTS, DeliveryTier::Observer, skipped(7)),
        ]);
        loss.record_display_dropped(4);

        assert_eq!((loss.total(), loss.priority_dropped()), (22, 7));
    }

    #[test]
    fn rows_put_priority_losses_first_then_larger_totals_then_names() {
        let mut loss = EventLoss::new();
        loss.apply_report(vec![
            lost(UI_EVENTS, DeliveryTier::Observer, skipped(50)),
            lost(VIEWER_TRACKER, DeliveryTier::Critical, skipped(3)),
            lost(EVENT_LOG, DeliveryTier::Critical, priority(1)),
            lost(TRIGGER_EVALUATOR, DeliveryTier::Critical, priority(4)),
            lost(RUN_HISTORY, DeliveryTier::Critical, skipped(3)),
        ]);

        assert_eq!(
            row_names(&loss),
            [
                TRIGGER_EVALUATOR,
                EVENT_LOG,
                UI_EVENTS,
                RUN_HISTORY,
                VIEWER_TRACKER
            ]
        );
    }

    #[test]
    fn dropped_chat_messages_show_as_their_own_display_row_that_accumulates() {
        let mut loss = EventLoss::new();
        loss.record_display_dropped(2);
        loss.record_display_dropped(3);

        let rows = loss.rows();
        assert_eq!(
            rows,
            [lost(UI_CHAT_DISPLAY, DeliveryTier::Observer, skipped(5))]
        );
    }

    #[test]
    fn recording_zero_dropped_messages_reports_no_change_and_adds_no_row() {
        let mut loss = EventLoss::new();

        assert_eq!(
            (loss.record_display_dropped(0), loss.rows().len()),
            (false, 0)
        );
    }

    #[test]
    fn an_identical_report_reports_no_change_and_a_different_one_does() {
        let report = || vec![lost(EVENT_LOG, DeliveryTier::Critical, priority(1))];
        let mut loss = EventLoss::new();
        loss.apply_report(report());

        let same = loss.apply_report(report());
        let grown = loss.apply_report(vec![lost(EVENT_LOG, DeliveryTier::Critical, priority(2))]);

        assert_eq!((same, grown), (false, true));
    }

    #[test]
    fn a_new_runtime_report_never_clears_dropped_chat_messages() {
        let mut loss = EventLoss::new();
        loss.record_display_dropped(3);
        loss.apply_report(vec![lost(EVENT_LOG, DeliveryTier::Critical, priority(1))]);

        loss.apply_report(Vec::new());

        assert_eq!(loss.total(), 3);
    }

    #[test]
    fn breakdown_lists_only_the_nonzero_kinds_in_lane_order() {
        crate::i18n::install_language(forge_storage::Language::En);
        let plain = |count: LossCount| breakdown(&count).replace(['\u{2068}', '\u{2069}'], "");

        assert_eq!(
            [
                plain(LossCount {
                    priority_dropped: 1,
                    bulk_dropped: 0,
                    skipped: 4,
                    unwritten: 2,
                }),
                plain(LossCount {
                    priority_dropped: 0,
                    bulk_dropped: 6,
                    skipped: 0,
                    unwritten: 0,
                }),
            ],
            [
                "1 priority dropped · 4 skipped · 2 not saved",
                "6 bulk dropped",
            ]
        );
    }

    #[test]
    fn an_unknown_consumer_is_labelled_by_its_name_with_spaces() {
        assert_eq!(consumer_label("custom_script_sink"), "custom script sink");
    }
}
