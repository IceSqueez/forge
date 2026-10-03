use forge_events::{Event, EventSource};
use forge_platform_core::ConnectionState;
use forge_types::IntegrationId;

use crate::home_stats::Integration;
use crate::integrations::BuiltinRegistry;

const ROSTER: [Integration; 5] = [
    Integration::Twitch,
    Integration::YouTube,
    Integration::Kick,
    Integration::Obs,
    Integration::VTube,
];

pub struct PlatformConnectivity {
    connections: Vec<(Integration, bool)>,
    services: Vec<(IntegrationId, bool)>,
    obs_password_rejected: bool,
}

impl Default for PlatformConnectivity {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformConnectivity {
    pub fn new() -> Self {
        Self {
            connections: ROSTER.iter().map(|integ| (*integ, false)).collect(),
            services: Vec::new(),
            obs_password_rejected: false,
        }
    }

    pub fn obs_password_rejected(&self) -> bool {
        self.obs_password_rejected
    }

    pub fn apply_obs_event(&mut self, event: &Event) -> bool {
        if event.source != EventSource::Obs {
            return false;
        }
        let rejected = match event.kind.as_str() {
            "obs.connection.auth_failed" => true,
            "obs.connection.connected" => false,
            _ => return false,
        };
        std::mem::replace(&mut self.obs_password_rejected, rejected) != rejected
    }

    pub fn connections(&self) -> &[(Integration, bool)] {
        &self.connections
    }

    pub fn is_connected(&self, integ: Integration) -> bool {
        self.connections
            .iter()
            .find(|(i, _)| *i == integ)
            .map(|(_, connected)| *connected)
            .unwrap_or(false)
    }

    pub fn is_integration_connected(&self, id: &IntegrationId) -> bool {
        match Integration::from_id(id.as_str()) {
            Some(integ) => self.is_connected(integ),
            None => self
                .services
                .iter()
                .any(|(service, connected)| service == id && *connected),
        }
    }

    pub fn set_service_connected(&mut self, id: &IntegrationId, connected: bool) -> bool {
        match self.services.iter_mut().find(|(service, _)| service == id) {
            Some(entry) if entry.1 == connected => false,
            Some(entry) => {
                entry.1 = connected;
                true
            }
            None => {
                self.services.push((id.clone(), connected));
                connected
            }
        }
    }

    pub fn set_connected(&mut self, integ: Integration, connected: bool) -> bool {
        if let Some(entry) = self.connections.iter_mut().find(|(i, _)| *i == integ)
            && entry.1 != connected
        {
            entry.1 = connected;
            return true;
        }
        false
    }

    pub fn tally_enabled(&self, is_enabled: impl Fn(Integration) -> bool) -> (usize, usize) {
        self.connections
            .iter()
            .filter(|(integ, _)| is_enabled(*integ))
            .fold((0, 0), |(connected, total), (_, ok)| {
                (connected + usize::from(*ok), total + 1)
            })
    }

    pub fn seed_from_builtins(&mut self, builtins: &BuiltinRegistry) {
        for (integ, connected) in self.connections.iter_mut() {
            *connected = builtins
                .get(&integ.builtin_id())
                .map(|obj| obj.status.connection() == ConnectionState::Connected)
                .unwrap_or(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_connected_tally_counts_only_enabled_integrations() {
        let mut connectivity = PlatformConnectivity::new();
        connectivity.set_connected(Integration::Twitch, true);
        connectivity.set_connected(Integration::Obs, true);
        let enabled = |on: &'static [Integration]| move |integ: Integration| on.contains(&integ);

        for (on, expected, case) in [
            (&[][..], (0, 0), "everything switched off"),
            (
                &[Integration::Twitch, Integration::Kick][..],
                (1, 2),
                "one of two enabled is connected",
            ),
            (
                &[Integration::Kick][..],
                (0, 1),
                "a connected integration that is switched off is not counted",
            ),
        ] {
            assert_eq!(connectivity.tally_enabled(enabled(on)), expected, "{case}");
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
                connectivity.set_service_connected(&donatello(), connected),
                changed,
                "{case}"
            );
            assert_eq!(
                connectivity.is_integration_connected(&donatello()),
                connected,
                "{case}"
            );
        }
    }

    #[test]
    fn a_first_sighting_while_online_is_reported_as_a_change() {
        let mut connectivity = PlatformConnectivity::new();

        assert!(connectivity.set_service_connected(&donatello(), true));
    }

    #[test]
    fn a_service_never_reported_counts_as_offline() {
        let mut connectivity = PlatformConnectivity::new();
        connectivity.set_service_connected(&IntegrationId::new("monobank"), true);

        assert!(!connectivity.is_integration_connected(&donatello()));
    }

    #[test]
    fn a_roster_integration_answers_from_the_roster_not_from_service_reports() {
        let twitch = IntegrationId::new("twitch");
        let mut connectivity = PlatformConnectivity::new();

        connectivity.set_service_connected(&twitch, true);
        assert!(
            !connectivity.is_integration_connected(&twitch),
            "a service report must not override the roster"
        );

        connectivity.set_connected(Integration::Twitch, true);
        assert!(connectivity.is_integration_connected(&twitch));
    }
}
