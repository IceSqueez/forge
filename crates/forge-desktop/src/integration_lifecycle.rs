use std::collections::HashSet;

use forge_platform_core::ConnectionAffordance;
use forge_types::IntegrationId;

use crate::integration_supervisor::{LifecycleState, LifecycleStates};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardStatus {
    Disabled,
    Starting,
    Stopping,
    Failed(String),
    Active,
    Connected,
    NotConnected,
}

impl CardStatus {
    pub fn resolve(
        state: &LifecycleState,
        connection: ConnectionAffordance,
        connected: bool,
    ) -> Self {
        match state {
            LifecycleState::Disabled => Self::Disabled,
            LifecycleState::Starting => Self::Starting,
            LifecycleState::Stopping => Self::Stopping,
            LifecycleState::Failed(reason) => Self::Failed(reason.clone()),
            LifecycleState::Running => match (connection, connected) {
                (ConnectionAffordance::Connectionless, _) => Self::Active,
                (ConnectionAffordance::Connectable, true) => Self::Connected,
                (ConnectionAffordance::Connectable, false) => Self::NotConnected,
            },
        }
    }
}

pub struct IntegrationLifecycle {
    states: LifecycleStates,
}

impl IntegrationLifecycle {
    pub fn new(states: LifecycleStates) -> Self {
        Self { states }
    }

    pub fn replace(&mut self, states: LifecycleStates) -> bool {
        if self.states == states {
            return false;
        }
        self.states = states;
        true
    }

    pub fn state_of(&self, id: &IntegrationId) -> LifecycleState {
        self.states
            .get(id)
            .cloned()
            .unwrap_or(LifecycleState::Disabled)
    }

    pub fn is_known(&self, id: &IntegrationId) -> bool {
        self.states.contains_key(id)
    }

    pub fn is_on(&self, id: &IntegrationId) -> bool {
        is_desired_on(&self.state_of(id))
    }

    pub fn is_switched_off(&self, id: &IntegrationId) -> bool {
        self.is_known(id) && !self.is_on(id)
    }

    pub fn switched_off(&self) -> HashSet<IntegrationId> {
        self.states
            .iter()
            .filter(|(_, state)| !is_desired_on(state))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn on_count<'a>(&self, ids: impl Iterator<Item = &'a IntegrationId>) -> usize {
        ids.filter(|id| self.is_on(id)).count()
    }
}

pub fn is_desired_on(state: &LifecycleState) -> bool {
    matches!(
        state,
        LifecycleState::Starting | LifecycleState::Running | LifecycleState::Failed(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAILURE: &str = "the token was revoked";

    fn failed() -> LifecycleState {
        LifecycleState::Failed(FAILURE.to_owned())
    }

    #[test]
    fn a_running_integration_reports_its_connection_only_when_it_has_one() {
        for (connection, connected, expected) in [
            (
                ConnectionAffordance::Connectable,
                true,
                CardStatus::Connected,
            ),
            (
                ConnectionAffordance::Connectable,
                false,
                CardStatus::NotConnected,
            ),
            (
                ConnectionAffordance::Connectionless,
                true,
                CardStatus::Active,
            ),
            (
                ConnectionAffordance::Connectionless,
                false,
                CardStatus::Active,
            ),
        ] {
            assert_eq!(
                CardStatus::resolve(&LifecycleState::Running, connection, connected),
                expected,
                "{connection:?} connected={connected}"
            );
        }
    }

    #[test]
    fn a_lifecycle_state_other_than_running_hides_a_live_connection() {
        for (state, expected) in [
            (LifecycleState::Disabled, CardStatus::Disabled),
            (LifecycleState::Starting, CardStatus::Starting),
            (LifecycleState::Stopping, CardStatus::Stopping),
            (failed(), CardStatus::Failed(FAILURE.to_owned())),
        ] {
            assert_eq!(
                CardStatus::resolve(&state, ConnectionAffordance::Connectable, true),
                expected
            );
        }
    }

    #[test]
    fn the_switch_stays_on_while_the_user_still_wants_the_integration() {
        for (state, on) in [
            (LifecycleState::Disabled, false),
            (LifecycleState::Starting, true),
            (LifecycleState::Running, true),
            (LifecycleState::Stopping, false),
            (failed(), true),
        ] {
            assert_eq!(is_desired_on(&state), on, "{state:?}");
        }
    }

    #[test]
    fn an_integration_absent_from_the_lifecycle_counts_as_off() {
        let twitch = IntegrationId::new("twitch");
        let kick = IntegrationId::new("kick");
        let obs = IntegrationId::new("obs");
        let lifecycle = IntegrationLifecycle::new(LifecycleStates::from([
            (twitch.clone(), LifecycleState::Running),
            (kick.clone(), LifecycleState::Disabled),
        ]));

        assert_eq!(lifecycle.on_count([&twitch, &kick, &obs].into_iter()), 1);
        assert_eq!(lifecycle.state_of(&obs), LifecycleState::Disabled);
    }

    #[test]
    fn replacing_the_states_reports_a_change_only_when_they_differ() {
        let twitch = IntegrationId::new("twitch");
        let running = LifecycleStates::from([(twitch.clone(), LifecycleState::Running)]);
        let mut lifecycle = IntegrationLifecycle::new(running.clone());

        assert!(!lifecycle.replace(running));
        assert!(lifecycle.replace(LifecycleStates::from([(twitch.clone(), failed())])));
        assert_eq!(lifecycle.state_of(&twitch), failed());
    }
}
