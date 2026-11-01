use std::sync::Arc;

use forge_platform_core::{BuiltinStatus, ConnectionState, RevisionWait};
use forge_types::IntegrationId;
use gpui::{App, Entity};

use crate::app::runtime_event::RuntimeEvent;
use crate::integrations::integration_catalog::connection_roster;
use forge_host::integrations::BuiltinRegistry;

pub(crate) struct PlatformConnectivity {
    connections: Vec<(IntegrationId, bool)>,
    roster_len: usize,
}

impl Default for PlatformConnectivity {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformConnectivity {
    pub(crate) fn new() -> Self {
        let connections: Vec<(IntegrationId, bool)> = connection_roster()
            .into_iter()
            .map(|id| (id, false))
            .collect();
        Self {
            roster_len: connections.len(),
            connections,
        }
    }

    pub(crate) fn apply_runtime_event(&mut self, event: &RuntimeEvent) -> bool {
        match event {
            RuntimeEvent::PlatformConnection(change) => {
                let id = IntegrationId::new(change.platform_id.as_str());
                let on_the_roster = self.connections().iter().any(|(entry, _)| *entry == id);
                if on_the_roster {
                    self.set_connected(&id, change.state.is_connected());
                }
                on_the_roster
            }
            _ => false,
        }
    }

    pub(crate) fn connections(&self) -> &[(IntegrationId, bool)] {
        &self.connections[..self.roster_len]
    }

    pub(crate) fn is_connected(&self, id: &IntegrationId) -> bool {
        self.connections
            .iter()
            .any(|(entry, connected)| entry == id && *connected)
    }

    pub(crate) fn set_connected(&mut self, id: &IntegrationId, connected: bool) -> bool {
        match self.connections.iter_mut().find(|(entry, _)| entry == id) {
            Some(entry) if entry.1 == connected => false,
            Some(entry) => {
                entry.1 = connected;
                true
            }
            None => {
                self.connections.push((id.clone(), connected));
                connected
            }
        }
    }

    pub(crate) fn tally_enabled(
        &self,
        is_enabled: impl Fn(&IntegrationId) -> bool,
    ) -> (usize, usize) {
        self.connections()
            .iter()
            .filter(|(id, _)| is_enabled(id))
            .fold((0, 0), |(connected, total), (_, ok)| {
                (connected + usize::from(*ok), total + 1)
            })
    }

    pub(crate) fn seed_from_builtins(&mut self, builtins: &BuiltinRegistry) {
        let roster_len = self.roster_len;
        for (id, connected) in self.connections[..roster_len].iter_mut() {
            *connected = builtins
                .get(id)
                .map(|obj| obj.status.connection() == ConnectionState::Connected)
                .unwrap_or(false);
        }
    }
}

pub(crate) fn apply_connection(
    connectivity: &Entity<PlatformConnectivity>,
    id: &IntegrationId,
    status: &dyn BuiltinStatus,
    cx: &mut App,
) {
    let connected = status.connection() == ConnectionState::Connected;
    connectivity.update(cx, |connectivity, cx| {
        if connectivity.set_connected(id, connected) {
            cx.notify();
        }
    });
}

pub(crate) fn mirror_status_connectivity(
    status: Arc<dyn BuiltinStatus>,
    connectivity: &Entity<PlatformConnectivity>,
    cx: &mut App,
) {
    let Some(mut revisions) = status.status_revisions() else {
        return;
    };
    let id = status.id().clone();
    let connectivity = connectivity.clone();
    apply_connection(&connectivity, &id, status.as_ref(), cx);
    cx.spawn(async move |cx| {
        while revisions.changed().await == RevisionWait::Changed {
            cx.update(|cx| apply_connection(&connectivity, &id, status.as_ref(), cx));
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_types::PlatformId;

    #[test]
    fn the_connected_tally_counts_only_enabled_integrations() {
        let mut connectivity = PlatformConnectivity::new();
        connectivity.set_connected(&PlatformId::Twitch.integration_id(), true);
        connectivity.set_connected(&forge_obs::OBS_INTEGRATION.id, true);
        let enabled = |on: Vec<IntegrationId>| move |integ: &IntegrationId| on.contains(integ);

        for (on, expected, case) in [
            (&[][..], (0, 0), "everything switched off"),
            (
                &[
                    PlatformId::Twitch.integration_id(),
                    PlatformId::Kick.integration_id(),
                ][..],
                (1, 2),
                "one of two enabled is connected",
            ),
            (
                &[PlatformId::Kick.integration_id()][..],
                (0, 1),
                "a connected integration that is switched off is not counted",
            ),
        ] {
            assert_eq!(
                connectivity.tally_enabled(enabled(on.to_vec())),
                expected,
                "{case}"
            );
        }
    }

    fn donatello() -> IntegrationId {
        IntegrationId::new("donatello")
    }

    #[test]
    fn a_service_reports_a_change_only_when_its_connection_flips() {
        let mut connectivity = PlatformConnectivity::new();

        let steps = [
            (
                false,
                false,
                "first sighting while offline changes nothing visible",
            ),
            (true, true, "coming online is a change"),
            (true, false, "repeating the same state is not a change"),
            (false, true, "dropping offline is a change"),
            (false, false, "staying offline is not a change"),
        ];

        for (connected, changed, case) in steps {
            assert_eq!(
                connectivity.set_connected(&donatello(), connected),
                changed,
                "{case}"
            );
            assert_eq!(connectivity.is_connected(&donatello()), connected, "{case}");
        }
    }

    #[test]
    fn a_first_sighting_while_online_is_reported_as_a_change() {
        let mut connectivity = PlatformConnectivity::new();

        assert!(connectivity.set_connected(&donatello(), true));
    }

    #[test]
    fn a_service_never_reported_counts_as_offline() {
        let mut connectivity = PlatformConnectivity::new();
        connectivity.set_connected(&IntegrationId::new("monobank"), true);

        assert!(!connectivity.is_connected(&donatello()));
    }

    #[test]
    fn a_service_outside_the_roster_stays_off_the_connection_strip() {
        let mut connectivity = PlatformConnectivity::new();
        let roster: Vec<IntegrationId> = connectivity
            .connections()
            .iter()
            .map(|(id, _)| id.clone())
            .collect();

        connectivity.set_connected(&donatello(), true);

        let shown: Vec<IntegrationId> = connectivity
            .connections()
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(shown, roster);
        assert!(connectivity.is_connected(&donatello()));
    }

    fn connection_event(platform_id: &str, state: ConnectionState) -> RuntimeEvent {
        RuntimeEvent::PlatformConnection(forge_events::PlatformConnectionChanged {
            platform_id: platform_id.to_owned(),
            state,
            expected: false,
        })
    }

    #[test]
    fn a_roster_connection_event_always_reports_a_change_so_a_new_failure_reason_repaints() {
        let mut connectivity = PlatformConnectivity::new();

        for (state, connected, case) in [
            (ConnectionState::Disconnected, false, "unchanged offline"),
            (ConnectionState::Reconnecting, false, "offline to retrying"),
            (ConnectionState::Connected, true, "coming online"),
            (ConnectionState::Connected, true, "unchanged online"),
        ] {
            let changed = connectivity.apply_runtime_event(&connection_event("obs", state));

            assert!(changed, "{case}");
            assert_eq!(
                connectivity.is_connected(&forge_obs::OBS_INTEGRATION.id),
                connected,
                "{case}"
            );
        }
    }

    #[test]
    fn a_connection_event_for_a_platform_outside_the_roster_changes_nothing() {
        let mut connectivity = PlatformConnectivity::new();

        let changed = connectivity
            .apply_runtime_event(&connection_event("donatello", ConnectionState::Connected));

        assert!(!changed);
        assert!(!connectivity.is_connected(&donatello()));
    }
}
