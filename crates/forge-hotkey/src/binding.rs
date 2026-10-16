use forge_platform_core::{BuiltinHealth, HealthValue};
use forge_types::{ActionId, TriggerInstanceId};

use crate::backend::HotkeyId;
use crate::client::HotkeyClient;
use crate::event_kinds;

pub const COMBO_FIELD: &str = "combo";
const CONFLICTS_METRIC: &str = "CONFLICTS";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HotkeyEdge {
    Press,
    Release,
}

impl HotkeyEdge {
    pub fn kind_id(self) -> &'static str {
        match self {
            HotkeyEdge::Press => event_kinds::GLOBAL_PRESSED,
            HotkeyEdge::Release => event_kinds::GLOBAL_RELEASED,
        }
    }

    pub fn from_kind(kind_id: &str) -> Option<Self> {
        match kind_id {
            event_kinds::GLOBAL_PRESSED => Some(HotkeyEdge::Press),
            event_kinds::GLOBAL_RELEASED => Some(HotkeyEdge::Release),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            HotkeyEdge::Press => HotkeyEdge::Release,
            HotkeyEdge::Release => HotkeyEdge::Press,
        }
    }
}

pub struct BindingHalf {
    pub instance_id: TriggerInstanceId,
    pub enabled: bool,
    pub action: Option<(ActionId, String)>,
}

pub struct BindingRow {
    pub key: TriggerInstanceId,
    pub combo: String,
    pub registered: bool,
    pub press: Option<BindingHalf>,
    pub release: Option<BindingHalf>,
}

impl BindingRow {
    pub fn half(&self, edge: HotkeyEdge) -> Option<&BindingHalf> {
        match edge {
            HotkeyEdge::Press => self.press.as_ref(),
            HotkeyEdge::Release => self.release.as_ref(),
        }
    }

    pub fn is_hold(&self) -> bool {
        self.press.is_some() && self.release.is_some()
    }

    pub fn is_release_edge(&self) -> bool {
        self.press.is_none() && self.release.is_some()
    }

    pub fn enabled(&self) -> bool {
        self.halves().all(|(_, half)| half.enabled)
    }

    pub fn halves(&self) -> impl Iterator<Item = (HotkeyEdge, &BindingHalf)> {
        [
            (HotkeyEdge::Press, self.press.as_ref()),
            (HotkeyEdge::Release, self.release.as_ref()),
        ]
        .into_iter()
        .filter_map(|(edge, half)| half.map(|half| (edge, half)))
    }

    pub fn primary_action(&self) -> Option<&(ActionId, String)> {
        self.press
            .as_ref()
            .or(self.release.as_ref())
            .and_then(|half| half.action.as_ref())
    }

    pub fn free_edge(&self) -> Option<HotkeyEdge> {
        match (self.press.is_some(), self.release.is_some()) {
            (true, false) => Some(HotkeyEdge::Release),
            (false, true) => Some(HotkeyEdge::Press),
            _ => None,
        }
    }
}

pub fn combo_keys(combo: &str) -> Vec<&str> {
    combo.split('+').filter(|part| !part.is_empty()).collect()
}

pub fn registered_combos(client: &HotkeyClient) -> Vec<(HotkeyId, String)> {
    client
        .registered_combos()
        .into_iter()
        .map(|(id, combo)| (id, combo.as_str().to_owned()))
        .collect()
}

pub fn conflict_count(client: &HotkeyClient) -> usize {
    client
        .metrics()
        .into_iter()
        .find(|metric| metric.label == CONFLICTS_METRIC)
        .and_then(|metric| match metric.value {
            HealthValue::Text { primary, .. } => primary.parse::<usize>().ok(),
            _ => None,
        })
        .unwrap_or(0)
}
