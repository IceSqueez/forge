use forge_types::IntegrationId;

use crate::integration_lifecycle::IntegrationLifecycle;
use crate::navigation::Screen;
use forge_runtime::LifecycleState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailRoute {
    Disabled,
    Failed,
    Live,
}

impl DetailRoute {
    pub(crate) fn resolve(lifecycle: &IntegrationLifecycle, id: &IntegrationId) -> Self {
        if lifecycle.is_switched_off(id) {
            Self::Disabled
        } else if matches!(lifecycle.state_of(id), LifecycleState::Failed(_)) {
            Self::Failed
        } else {
            Self::Live
        }
    }
}

pub(crate) fn is_detail_of(current: &Screen, id: &IntegrationId) -> bool {
    matches!(current, Screen::BuiltinDetail(shown) if shown == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_runtime::LifecycleStates;

    #[test]
    fn the_detail_route_follows_the_integration_lifecycle_state() {
        let cases = [
            ("disabled", LifecycleState::Disabled, DetailRoute::Disabled),
            ("stopping", LifecycleState::Stopping, DetailRoute::Disabled),
            (
                "failed",
                LifecycleState::Failed(crate::test_support::start_failure("token revoked")),
                DetailRoute::Failed,
            ),
            ("starting", LifecycleState::Starting, DetailRoute::Live),
            ("running", LifecycleState::Running, DetailRoute::Live),
        ];
        let lifecycle = IntegrationLifecycle::new(
            cases
                .iter()
                .map(|(id, state, _)| (IntegrationId::new(*id), state.clone()))
                .collect::<LifecycleStates>(),
        );

        for (id, state, expected) in cases {
            assert_eq!(
                DetailRoute::resolve(&lifecycle, &IntegrationId::new(id)),
                expected,
                "{state:?}"
            );
        }
    }

    #[test]
    fn an_integration_the_lifecycle_does_not_know_routes_to_the_live_detail() {
        let lifecycle = IntegrationLifecycle::new(LifecycleStates::new());

        assert_eq!(
            DetailRoute::resolve(&lifecycle, &IntegrationId::new("plugin")),
            DetailRoute::Live
        );
    }

    #[test]
    fn only_the_detail_screen_of_the_same_integration_counts_as_its_detail() {
        let obs = IntegrationId::new("obs");

        assert_eq!(
            [
                is_detail_of(&Screen::BuiltinDetail(obs.clone()), &obs),
                is_detail_of(&Screen::BuiltinDetail(IntegrationId::new("kick")), &obs),
                is_detail_of(&Screen::Integrations(None), &obs),
            ],
            [true, false, false]
        );
    }
}
