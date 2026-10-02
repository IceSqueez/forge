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
