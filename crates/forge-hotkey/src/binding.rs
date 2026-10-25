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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_half(action: Option<(ActionId, &str)>) -> BindingHalf {
        BindingHalf {
            instance_id: TriggerInstanceId::new(),
            enabled: true,
            action: action.map(|(id, name)| (id, name.to_owned())),
        }
    }

    fn test_row(press: Option<BindingHalf>, release: Option<BindingHalf>) -> BindingRow {
        let key = press
            .as_ref()
            .or(release.as_ref())
            .map(|half| half.instance_id)
            .expect("a row always carries at least one half");
        BindingRow {
            key,
            combo: "Ctrl+F1".to_owned(),
            registered: true,
            press,
            release,
        }
    }

    #[test]
    fn combo_keys_yields_one_cap_per_token_and_never_an_empty_cap() {
        let cases: [(&str, Vec<&str>); 4] = [
            ("Ctrl+Shift+F5", vec!["Ctrl", "Shift", "F5"]),
            ("A", vec!["A"]),
            ("Ctrl+", vec!["Ctrl"]),
            ("", vec![]),
        ];

        for (combo, expected) in cases {
            assert_eq!(combo_keys(combo), expected, "wrong caps for {combo:?}");
        }
    }

    #[test]
    fn binding_row_shape_drives_the_badge_and_the_free_edge() {
        let cases = [
            (
                "press only",
                test_row(Some(test_half(None)), None),
                (false, false, Some(HotkeyEdge::Release)),
            ),
            (
                "release only",
                test_row(None, Some(test_half(None))),
                (false, true, Some(HotkeyEdge::Press)),
            ),
            (
                "both halves",
                test_row(Some(test_half(None)), Some(test_half(None))),
                (true, false, None),
            ),
        ];

        for (case, row, expected) in cases {
            assert_eq!(
                (row.is_hold(), row.is_release_edge(), row.free_edge()),
                expected,
                "wrong (hold, release-edge, free-edge) for a {case} row"
            );
        }
    }

    #[test]
    fn binding_row_primary_action_reads_the_press_half_whenever_the_row_has_one() {
        let start = ActionId::new();
        let stop = ActionId::new();
        let cases = [
            (
                "a hold reports the press half's action",
                test_row(
                    Some(test_half(Some((start, "Start")))),
                    Some(test_half(Some((stop, "Stop")))),
                ),
                Some((start, "Start".to_owned())),
            ),
            (
                "a release-only row reports its own action",
                test_row(None, Some(test_half(Some((stop, "Stop"))))),
                Some((stop, "Stop".to_owned())),
            ),
            (
                "an unlinked press half is not backfilled from the release half",
                test_row(Some(test_half(None)), Some(test_half(Some((stop, "Stop"))))),
                None,
            ),
        ];

        for (case, row, expected) in cases {
            assert_eq!(row.primary_action().cloned(), expected, "wrong when {case}");
        }
    }
}
