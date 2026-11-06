use crate::vocabulary::{RaidDirection, messages};
use forge_platform_core::{CollectionRevisionSignal, CollectionRevisions};
use forge_types::{RecoverRwLock, UserText};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubStatus {
    Pending,
    Active,
    Retrying(SubscriptionFailure),
    Failed(SubscriptionFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionFailure {
    Unauthorized,
    MissingScope,
    AlreadyExists,
    Refused(u16),
    UnreadableResponse,
    RateLimited,
    Transport,
    Credentials,
    Revoked(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionRecord {
    pub kind: String,
    pub direction: Option<RaidDirection>,
    pub version: String,
    pub status: SubStatus,
    pub subscription_id: Option<String>,
}

impl SubscriptionRecord {
    pub fn label(&self) -> UserText {
        messages::subscription_label(&self.kind, self.direction)
    }

    fn matches_wire(&self, kind: &str, direction: Option<RaidDirection>) -> bool {
        self.kind == kind && self.direction == direction
    }
}

#[derive(Debug, Default)]
struct TrackerState {
    records: RwLock<Vec<SubscriptionRecord>>,
    event_counts: RwLock<HashMap<String, u64>>,
    changes: CollectionRevisionSignal,
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionTracker(Arc<TrackerState>);

impl SubscriptionTracker {
    pub fn snapshot(&self) -> Vec<SubscriptionRecord> {
        self.read().clone()
    }

    pub fn active_count(&self) -> usize {
        self.read()
            .iter()
            .filter(|record| record.status == SubStatus::Active)
            .count()
    }

    pub fn reset(&self, records: Vec<SubscriptionRecord>) {
        let records_changed = {
            let mut current = self.write();
            let changed = *current != records;
            *current = records;
            changed
        };
        let counts_cleared = {
            let mut counts = self.0.event_counts.write_recover();
            let had_counts = !counts.is_empty();
            counts.clear();
            had_counts
        };
        if records_changed || counts_cleared {
            self.0.changes.bump();
        }
    }

    pub(crate) fn revisions(&self) -> CollectionRevisions {
        self.0.changes.subscribe()
    }

    pub(crate) fn is_active(&self, kind: &str) -> bool {
        self.read()
            .iter()
            .any(|record| record.kind == kind && record.status == SubStatus::Active)
    }

    pub(crate) fn lacks_a_scope(&self) -> bool {
        self.read()
            .iter()
            .any(|record| record.status == SubStatus::Failed(SubscriptionFailure::MissingScope))
    }

    pub(crate) fn record_event(&self, subscription_type: &str) {
        {
            let mut counts = self.0.event_counts.write_recover();
            match counts.get_mut(subscription_type) {
                Some(count) => *count = count.saturating_add(1),
                None => {
                    counts.insert(subscription_type.to_owned(), 1);
                }
            }
        }
        self.0.changes.bump();
    }

    pub(crate) fn event_counts(&self) -> HashMap<String, u64> {
        self.0.event_counts.read_recover().clone()
    }

    pub(crate) fn set_status(&self, index: usize, status: SubStatus) {
        self.update_record(index, |record| record.status = status);
    }

    pub(crate) fn mark_active(&self, index: usize, subscription_id: Option<String>) {
        self.update_record(index, |record| {
            record.status = SubStatus::Active;
            record.subscription_id = subscription_id;
        });
    }

    pub(crate) fn retrying_reason(&self, index: usize) -> Option<SubscriptionFailure> {
        match self.read().get(index).map(|record| &record.status) {
            Some(SubStatus::Retrying(reason)) => Some(reason.clone()),
            _ => None,
        }
    }

    pub(crate) fn settle_retrying(&self, index: usize) {
        self.update_record(index, |record| {
            if let SubStatus::Retrying(reason) = &record.status {
                record.status = SubStatus::Failed(reason.clone());
            }
        });
    }

    pub(crate) fn mark_revoked(
        &self,
        subscription_id: &str,
        kind: &str,
        direction: Option<RaidDirection>,
        status: &str,
    ) {
        let index = {
            let records = self.read();
            let by_id = records
                .iter()
                .position(|record| record.subscription_id.as_deref() == Some(subscription_id));
            by_id.or_else(|| {
                records
                    .iter()
                    .position(|record| record.matches_wire(kind, direction))
            })
        };
        if let Some(index) = index {
            self.update_record(index, |record| {
                record.status = SubStatus::Failed(SubscriptionFailure::Revoked(status.to_owned()));
            });
        }
    }

    fn update_record(&self, index: usize, update: impl FnOnce(&mut SubscriptionRecord)) {
        let changed = {
            let mut records = self.write();
            records.get_mut(index).is_some_and(|record| {
                let before = record.clone();
                update(record);
                *record != before
            })
        };
        if changed {
            self.0.changes.bump();
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, Vec<SubscriptionRecord>> {
        self.0.records.read_recover()
    }

    fn write(&self) -> RwLockWriteGuard<'_, Vec<SubscriptionRecord>> {
        self.0.records.write_recover()
    }
}

#[cfg(test)]
mod tests {
    use super::{SubStatus, SubscriptionFailure, SubscriptionRecord, SubscriptionTracker};
    use crate::vocabulary::RaidDirection;

    const RAID: &str = "channel.raid";

    fn active(kind: &str, direction: Option<RaidDirection>) -> SubscriptionRecord {
        SubscriptionRecord {
            kind: kind.to_owned(),
            direction,
            version: "1".to_owned(),
            status: SubStatus::Active,
            subscription_id: None,
        }
    }

    fn revoked() -> SubStatus {
        SubStatus::Failed(SubscriptionFailure::Revoked("user_removed".to_owned()))
    }

    fn statuses_after_revoking(
        records: Vec<SubscriptionRecord>,
        kind: &str,
        direction: Option<RaidDirection>,
    ) -> Vec<SubStatus> {
        let tracker = SubscriptionTracker::default();
        tracker.reset(records);
        tracker.mark_revoked("sub-unlisted", kind, direction, "user_removed");
        tracker
            .snapshot()
            .into_iter()
            .map(|record| record.status)
            .collect()
    }

    #[test]
    fn label_names_the_raid_direction_and_leaves_other_topics_bare() {
        for (direction, expected) in [
            (
                Some(RaidDirection::Received),
                "twitch.subscription_incoming",
            ),
            (Some(RaidDirection::Sent), "twitch.subscription_outgoing"),
            (None, RAID),
        ] {
            assert_eq!(active(RAID, direction).label().code_or_text(), expected);
        }
    }

    #[test]
    fn a_revocation_without_a_known_id_fails_only_the_row_of_its_direction() {
        let rows = || {
            vec![
                active(RAID, Some(RaidDirection::Received)),
                active(RAID, Some(RaidDirection::Sent)),
            ]
        };
        for (direction, expected) in [
            (
                Some(RaidDirection::Received),
                vec![revoked(), SubStatus::Active],
            ),
            (
                Some(RaidDirection::Sent),
                vec![SubStatus::Active, revoked()],
            ),
            (None, vec![SubStatus::Active, SubStatus::Active]),
        ] {
            assert_eq!(
                statuses_after_revoking(rows(), RAID, direction),
                expected,
                "{direction:?}"
            );
        }
    }

    #[test]
    fn a_revocation_of_an_untracked_kind_changes_no_row() {
        assert_eq!(
            statuses_after_revoking(vec![active("channel.follow", None)], "channel.cheer", None),
            vec![SubStatus::Active]
        );
    }

    fn moved(revisions: &mut forge_platform_core::CollectionRevisions) -> bool {
        futures_util::FutureExt::now_or_never(revisions.changed()).is_some()
    }

    #[test]
    fn every_real_tracker_change_moves_the_revision_and_a_repeat_does_not() {
        let tracker = SubscriptionTracker::default();
        let mut revisions = tracker.revisions();
        let records = vec![active("channel.follow", None)];

        tracker.reset(Vec::new());
        assert!(!moved(&mut revisions), "reset to the same empty list");

        tracker.reset(records.clone());
        assert!(moved(&mut revisions), "new list");

        tracker.reset(records);
        assert!(!moved(&mut revisions), "reset to the same list");

        tracker.set_status(0, SubStatus::Active);
        assert!(!moved(&mut revisions), "same status");

        tracker.set_status(1, SubStatus::Pending);
        assert!(!moved(&mut revisions), "no such row");

        tracker.mark_active(0, None);
        assert!(!moved(&mut revisions), "already active with that id");

        tracker.mark_active(0, Some("sub-1".to_owned()));
        assert!(moved(&mut revisions), "a subscription id arrived");

        tracker.settle_retrying(0);
        assert!(!moved(&mut revisions), "nothing was retrying");

        tracker.set_status(0, SubStatus::Retrying(SubscriptionFailure::Transport));
        assert!(moved(&mut revisions), "started retrying");

        tracker.settle_retrying(0);
        assert!(moved(&mut revisions), "retry settled as failed");

        tracker.mark_revoked("sub-1", "channel.follow", None, "user_removed");
        assert!(moved(&mut revisions), "revoked");

        tracker.mark_revoked("sub-1", "channel.follow", None, "user_removed");
        assert!(!moved(&mut revisions), "revoked the same way again");

        tracker.mark_revoked("sub-unknown", "channel.unlisted", None, "user_removed");
        assert!(!moved(&mut revisions), "no matching row");
    }

    #[test]
    fn events_are_counted_per_subscription_type_until_the_connection_resubscribes() {
        let tracker = SubscriptionTracker::default();
        let records = vec![active("channel.follow", None)];
        tracker.reset(records.clone());
        let mut revisions = tracker.revisions();

        tracker.record_event("channel.follow");
        assert!(moved(&mut revisions), "first event");
        tracker.record_event("channel.follow");
        tracker.record_event("channel.raid");
        assert!(moved(&mut revisions), "later events");

        let counts = tracker.event_counts();
        assert_eq!(counts.get("channel.follow"), Some(&2));
        assert_eq!(counts.get("channel.raid"), Some(&1));

        tracker.reset(records);
        assert!(tracker.event_counts().is_empty());
        assert!(
            moved(&mut revisions),
            "a reset that clears counts is a change"
        );
    }
}
