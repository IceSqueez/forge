use crate::vocabulary::RaidDirection;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubStatus {
    Pending,
    Active,
    Retrying(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    pub kind: String,
    pub direction: Option<RaidDirection>,
    pub version: String,
    pub status: SubStatus,
    pub subscription_id: Option<String>,
}

impl SubscriptionRecord {
    pub fn label(&self) -> String {
        match self.direction {
            Some(RaidDirection::Received) => format!("{} (incoming)", self.kind),
            Some(RaidDirection::Sent) => format!("{} (outgoing)", self.kind),
            None => self.kind.clone(),
        }
    }

    fn matches_wire(&self, kind: &str, direction: Option<RaidDirection>) -> bool {
        self.kind == kind && self.direction == direction
    }
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionTracker(Arc<RwLock<Vec<SubscriptionRecord>>>);

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
        *self.write() = records;
    }

    pub(crate) fn set_status(&self, index: usize, status: SubStatus) {
        if let Some(record) = self.write().get_mut(index) {
            record.status = status;
        }
    }

    pub(crate) fn mark_active(&self, index: usize, subscription_id: Option<String>) {
        if let Some(record) = self.write().get_mut(index) {
            record.status = SubStatus::Active;
            record.subscription_id = subscription_id;
        }
    }

    pub(crate) fn retrying_reason(&self, index: usize) -> Option<String> {
        match self.read().get(index).map(|record| &record.status) {
            Some(SubStatus::Retrying(reason)) => Some(reason.clone()),
            _ => None,
        }
    }

    pub(crate) fn settle_retrying(&self, index: usize) {
        if let Some(record) = self.write().get_mut(index)
            && let SubStatus::Retrying(reason) = &mut record.status
        {
            record.status = SubStatus::Failed(std::mem::take(reason));
        }
    }

    pub(crate) fn mark_revoked(
        &self,
        subscription_id: &str,
        kind: &str,
        direction: Option<RaidDirection>,
        status: &str,
    ) {
        let mut records = self.write();
        let by_id = records
            .iter()
            .position(|record| record.subscription_id.as_deref() == Some(subscription_id));
        let index = by_id.or_else(|| {
            records
                .iter()
                .position(|record| record.matches_wire(kind, direction))
        });
        if let Some(record) = index.and_then(|index| records.get_mut(index)) {
            record.status = SubStatus::Failed(format!("revoked: {status}"));
        }
    }

    fn read(&self) -> RwLockReadGuard<'_, Vec<SubscriptionRecord>> {
        self.0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, Vec<SubscriptionRecord>> {
        self.0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{SubStatus, SubscriptionRecord, SubscriptionTracker};
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
        SubStatus::Failed("revoked: user_removed".to_owned())
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
            (Some(RaidDirection::Received), "channel.raid (incoming)"),
            (Some(RaidDirection::Sent), "channel.raid (outgoing)"),
            (None, "channel.raid"),
        ] {
            assert_eq!(active(RAID, direction).label(), expected);
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
}
