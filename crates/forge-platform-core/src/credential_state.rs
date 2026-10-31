use std::sync::{Arc, RwLock};

use forge_types::RecoverRwLock;

use crate::collections::{CollectionRevisionSignal, CollectionRevisions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialState {
    Valid,
    ReauthRequired,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct CredentialStateTracker {
    state: Arc<RwLock<CredentialState>>,
    status_signal: Arc<CollectionRevisionSignal>,
}

impl CredentialStateTracker {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(CredentialState::Valid)),
            status_signal: Arc::new(CollectionRevisionSignal::new()),
        }
    }

    pub fn current(&self) -> CredentialState {
        *self.state.read_recover()
    }

    pub fn report(&self, state: CredentialState) {
        let previous = std::mem::replace(&mut *self.state.write_recover(), state);
        if previous != state {
            self.status_signal.bump();
        }
    }

    pub fn status_signal(&self) -> Arc<CollectionRevisionSignal> {
        Arc::clone(&self.status_signal)
    }

    pub fn revisions(&self) -> CollectionRevisions {
        self.status_signal.subscribe()
    }
}

impl Default for CredentialStateTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_change_bumps_the_revision_and_a_repeat_does_not() {
        let tracker = CredentialStateTracker::new();
        let mut revisions = tracker.revisions();

        tracker.report(CredentialState::Valid);
        assert!(!revisions.moved());

        for state in [
            CredentialState::ReauthRequired,
            CredentialState::Rejected,
            CredentialState::Valid,
        ] {
            tracker.report(state);
            assert_eq!(tracker.current(), state);
            assert!(revisions.moved(), "{state:?}");

            tracker.report(state);
            assert!(!revisions.moved(), "{state:?} repeated");
        }
    }

    #[test]
    fn clones_share_one_state_and_one_signal() {
        let tracker = CredentialStateTracker::new();
        let mut revisions = tracker.clone().revisions();

        tracker.clone().report(CredentialState::Rejected);

        assert_eq!(tracker.current(), CredentialState::Rejected);
        assert!(revisions.moved());
    }
}
