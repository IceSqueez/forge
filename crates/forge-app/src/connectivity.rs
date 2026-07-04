use forge_platform_core::{BuiltinId, ConnectionState};
use forge_types::PlatformId;
use forge_widgets::ForgePalette;
use iced::Color;

use crate::runtime_view::RuntimeView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integration {
    Twitch,
    YouTube,
    Kick,
    Obs,
    VTube,
}

pub const ROSTER: [Integration; 5] = [
    Integration::Twitch,
    Integration::YouTube,
    Integration::Kick,
    Integration::Obs,
    Integration::VTube,
];

impl Integration {
    pub fn label(self) -> &'static str {
        match self {
            Integration::Twitch => "Twitch",
            Integration::YouTube => "YouTube",
            Integration::Kick => "Kick",
            Integration::Obs => "OBS Studio",
            Integration::VTube => "VTube Studio",
        }
    }

    pub fn builtin_id(self) -> BuiltinId {
        BuiltinId::new(match self {
            Integration::Twitch => "twitch",
            Integration::YouTube => "youtube",
            Integration::Kick => "kick",
            Integration::Obs => "obs",
            Integration::VTube => "vtube",
        })
    }

    pub fn brand_color(self, palette: &ForgePalette) -> Color {
        match self {
            Integration::Twitch => palette.brand,
            Integration::YouTube => palette.random,
            Integration::Kick => palette.info,
            Integration::Obs => palette.success,
            Integration::VTube => palette.warning,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IntegrationStatus {
    pub integration: Integration,
    pub state: ConnectionState,
}

impl IntegrationStatus {
    pub fn is_connected(self) -> bool {
        matches!(self.state, ConnectionState::Connected)
    }
}

pub struct Connectivity {
    statuses: [IntegrationStatus; 5],
}

impl Connectivity {
    pub fn resolve(rt: &RuntimeView) -> Self {
        let statuses = ROSTER.map(|integration| IntegrationStatus {
            integration,
            state: resolve_state(rt, integration),
        });
        Self { statuses }
    }

    pub fn statuses(&self) -> &[IntegrationStatus] {
        &self.statuses
    }

    pub fn state(&self, integration: Integration) -> ConnectionState {
        self.statuses
            .iter()
            .find(|s| s.integration == integration)
            .map(|s| s.state)
            .unwrap_or(ConnectionState::Disconnected)
    }

    pub fn connected_count(&self) -> u8 {
        self.statuses.iter().filter(|s| s.is_connected()).count() as u8
    }

    pub fn total(&self) -> u8 {
        self.statuses.len() as u8
    }
}

pub fn state_color(state: ConnectionState, palette: &ForgePalette) -> Color {
    match state {
        ConnectionState::Connected => palette.success,
        ConnectionState::Connecting | ConnectionState::Reconnecting => palette.warning,
        ConnectionState::Disconnected => palette.text_faint,
    }
}

fn resolve_state(rt: &RuntimeView, integration: Integration) -> ConnectionState {
    match integration {
        Integration::Twitch => platform_state(rt, PlatformId::Twitch, rt.twitch_builtin.is_some()),
        Integration::YouTube => {
            platform_state(rt, PlatformId::YouTube, rt.youtube_builtin.is_some())
        }
        Integration::Kick => platform_state(rt, PlatformId::Kick, rt.kick_builtin.is_some()),
        Integration::Obs => rt
            .obs_client
            .as_ref()
            .map(|c| c.connection_state())
            .unwrap_or(ConnectionState::Disconnected),
        Integration::VTube => rt
            .vtube_client
            .as_ref()
            .map(|c| c.connection_state())
            .unwrap_or(ConnectionState::Disconnected),
    }
}

/// Kick never feeds `platform_connection` (its transient event is non-canonical),
/// so it relies entirely on the bundle-present fallback rather than a live entry.
fn platform_state(rt: &RuntimeView, id: PlatformId, bundle_present: bool) -> ConnectionState {
    rt.platform_connection
        .get(&id)
        .copied()
        .unwrap_or(if bundle_present {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        })
}
