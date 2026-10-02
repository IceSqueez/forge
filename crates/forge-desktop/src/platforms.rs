use forge_platform_core::ConnectionState;

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
        }
    }

    pub fn is_connected(&self, integ: Integration) -> bool {
        self.connections
            .iter()
            .find(|(i, _)| *i == integ)
            .map(|(_, connected)| *connected)
            .unwrap_or(false)
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
}
