use forge_types::IntegrationId;

use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::LifecycleState;
use crate::screen::Screen;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailRoute {
    Disabled,
    Failed,
    Live,
}

impl DetailRoute {
    pub fn resolve(lifecycle: &IntegrationLifecycle, id: &IntegrationId) -> Self {
        if lifecycle.is_switched_off(id) {
            Self::Disabled
        } else if matches!(lifecycle.state_of(id), LifecycleState::Failed(_)) {
            Self::Failed
        } else {
            Self::Live
        }
    }
}

pub fn is_detail_of(current: &Screen, id: &IntegrationId) -> bool {
    matches!(current, Screen::BuiltinDetail(shown) if shown == id)
}
