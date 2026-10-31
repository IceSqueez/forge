use std::sync::atomic::Ordering;

use forge_platform_core::ConnectionState;
use forge_types::RecoverMutex;
use time::OffsetDateTime;

use crate::client::HotkeyClient;
use crate::combo::HotkeyCombo;
use crate::health::TriggerRecord;

pub(crate) struct HotkeyView {
    pub(crate) enabled: bool,
    pub(crate) registered: Vec<HotkeyCombo>,
    pub(crate) conflicts: usize,
    pub(crate) backend_session_lost: Option<String>,
    pub(crate) last_triggered: Option<(OffsetDateTime, String)>,
    pub(crate) recent_triggers: Vec<TriggerRecord>,
}

impl HotkeyView {
    pub(crate) fn registered_count(&self) -> usize {
        self.registered.len()
    }

    pub(crate) fn connection(&self) -> ConnectionState {
        if self.enabled && self.backend_session_lost.is_none() && !self.registered.is_empty() {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        }
    }
}

impl HotkeyClient {
    pub(crate) fn view(&self) -> HotkeyView {
        let registered = self
            .registered_combos()
            .into_iter()
            .map(|(_, combo)| combo)
            .collect();
        let snap = self.health_state.lock_recover();
        HotkeyView {
            enabled: self.enabled.load(Ordering::Relaxed),
            registered,
            conflicts: snap.conflict_count,
            backend_session_lost: snap.backend_session_lost.clone(),
            last_triggered: snap
                .last_triggered_at
                .zip(snap.last_triggered_combo.clone()),
            recent_triggers: snap.recent_triggers.iter().rev().cloned().collect(),
        }
    }
}
