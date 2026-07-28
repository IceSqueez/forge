use std::collections::HashMap;
use std::sync::Arc;

use forge_storage::settings::reserved_keys::KEYBOARD_SHORTCUTS;
use forge_storage::{SettingsRepo, set_json_setting};
use gpui::{App, Keystroke};

use crate::actions::{
    SHORTCUTS, ShortcutEntry, canonical_chord, chord_is_bindable, effective_chord,
    parse_stored_overrides, reapply_key_bindings,
};

pub enum ChordVerdict {
    Unusable,
    NeedsModifier,
    Taken {
        owner_id: &'static str,
        chord: String,
    },
    Free(String),
}

#[derive(Default)]
pub struct ShortcutOverrides {
    map: HashMap<String, String>,
}

impl ShortcutOverrides {
    pub fn replace_stored(&mut self, raw: Option<&str>) {
        self.map = raw.map(parse_stored_overrides).unwrap_or_default();
    }

    pub fn chord_of(&self, entry: &'static ShortcutEntry) -> Option<&str> {
        effective_chord(&self.map, entry)
    }

    pub fn is_overridden(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    pub fn bound_count(&self) -> usize {
        SHORTCUTS
            .iter()
            .filter(|entry| self.chord_of(entry).is_some())
            .count()
    }

    pub fn owner_of(&self, chord: &str, exclude: &str) -> Option<&'static str> {
        SHORTCUTS
            .iter()
            .find(|entry| entry.id != exclude && effective_chord(&self.map, entry) == Some(chord))
            .map(|entry| entry.id)
    }

    pub fn verdict(&self, keystroke: &Keystroke, target_id: &str) -> ChordVerdict {
        let Some(chord) = canonical_chord(keystroke) else {
            return ChordVerdict::Unusable;
        };
        if !chord_is_bindable(&chord) {
            return ChordVerdict::NeedsModifier;
        }
        match self.owner_of(&chord, target_id) {
            Some(owner_id) => ChordVerdict::Taken { owner_id, chord },
            None => ChordVerdict::Free(chord),
        }
    }

    pub fn bind(&mut self, id: &str, chord: String) {
        let default = SHORTCUTS
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.default_chord);
        if default == Some(chord.as_str()) {
            self.map.remove(id);
        } else {
            self.map.insert(id.to_owned(), chord);
        }
    }

    /// Stores an unbindable chord, which `effective_chord` reads back as "no chord" without losing the entry.
    pub fn unbind(&mut self, id: &str) {
        self.map.insert(id.to_owned(), String::new());
    }

    pub fn reset(&mut self, id: &str) {
        self.map.remove(id);
    }

    pub fn reset_all(&mut self) {
        self.map.clear();
    }

    pub fn apply(&self, cx: &mut App) {
        reapply_key_bindings(cx, &self.map);
    }

    pub fn snapshot(&self) -> HashMap<String, String> {
        self.map.clone()
    }
}

pub async fn save_overrides(
    repo: Arc<dyn SettingsRepo>,
    map: HashMap<String, String>,
) -> Result<(), String> {
    set_json_setting(repo.as_ref(), KEYBOARD_SHORTCUTS, &map)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use gpui::Modifiers;

    use super::*;

    const HOME: &str = "nav.home";
    const HOME_DEFAULT: &str = "ctrl-1";
    const CHAT: &str = "nav.chat";
    const CHAT_DEFAULT: &str = "ctrl-2";

    fn entry(id: &str) -> &'static ShortcutEntry {
        crate::actions::shortcut_entry(id).expect("roster entry")
    }

    fn chord(overrides: &ShortcutOverrides, id: &str) -> Option<String> {
        overrides.chord_of(entry(id)).map(str::to_owned)
    }

    fn keystroke(modifiers: Modifiers, key: &str) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    fn ctrl(key: &str) -> Keystroke {
        keystroke(
            Modifiers {
                control: true,
                ..Default::default()
            },
            key,
        )
    }

    #[test]
    fn a_fresh_override_map_reports_every_roster_default() {
        let overrides = ShortcutOverrides::default();

        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
        assert_eq!(overrides.bound_count(), SHORTCUTS.len());
        assert!(!overrides.is_overridden(HOME));
    }

    #[test]
    fn binding_a_shortcut_back_to_its_default_clears_the_override_instead_of_storing_it() {
        let mut overrides = ShortcutOverrides::default();

        overrides.bind(HOME, "ctrl-9".to_owned());
        assert!(overrides.is_overridden(HOME));

        overrides.bind(HOME, HOME_DEFAULT.to_owned());

        assert!(
            !overrides.is_overridden(HOME),
            "a default-valued override must not be persisted"
        );
        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
    }

    #[test]
    fn unbind_leaves_the_shortcut_without_a_chord_while_reset_restores_the_default() {
        let mut overrides = ShortcutOverrides::default();

        overrides.unbind(HOME);
        assert_eq!(
            chord(&overrides, HOME),
            None,
            "unbind must not fall back to the default"
        );
        assert!(overrides.is_overridden(HOME));
        assert_eq!(overrides.bound_count(), SHORTCUTS.len() - 1);

        overrides.reset(HOME);
        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
        assert!(!overrides.is_overridden(HOME));
    }

    #[test]
    fn reset_all_drops_every_override_at_once() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());
        overrides.unbind(CHAT);

        overrides.reset_all();

        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
        assert_eq!(chord(&overrides, CHAT).as_deref(), Some(CHAT_DEFAULT));
        assert!(overrides.snapshot().is_empty());
    }

    #[test]
    fn owner_of_finds_a_shortcut_holding_the_chord_by_default_not_only_by_override() {
        let overrides = ShortcutOverrides::default();

        assert_eq!(overrides.owner_of(CHAT_DEFAULT, HOME), Some(CHAT));
        assert_eq!(
            overrides.owner_of(CHAT_DEFAULT, CHAT),
            None,
            "the excluded shortcut must not be reported as its own owner"
        );
        assert_eq!(overrides.owner_of("ctrl-9", HOME), None);
    }

    #[test]
    fn owner_of_follows_a_chord_that_moved_to_another_shortcut() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(CHAT, "ctrl-9".to_owned());

        assert_eq!(overrides.owner_of("ctrl-9", HOME), Some(CHAT));
        assert_eq!(
            overrides.owner_of(CHAT_DEFAULT, HOME),
            None,
            "the vacated default must no longer read as taken"
        );
    }

    #[test]
    fn verdict_rejects_a_keystroke_that_cannot_become_a_chord() {
        let overrides = ShortcutOverrides::default();

        for keystroke in [ctrl(""), ctrl("ctrl"), keystroke(Modifiers::default(), "")] {
            assert!(matches!(
                overrides.verdict(&keystroke, HOME),
                ChordVerdict::Unusable
            ));
        }
    }

    #[test]
    fn verdict_demands_a_strong_modifier_unless_the_key_is_a_function_key() {
        let overrides = ShortcutOverrides::default();
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };

        assert!(matches!(
            overrides.verdict(&keystroke(shift, "r"), HOME),
            ChordVerdict::NeedsModifier
        ));
        assert!(matches!(
            overrides.verdict(&keystroke(Modifiers::default(), "f13"), HOME),
            ChordVerdict::NeedsModifier
        ));
        assert!(matches!(
            overrides.verdict(&keystroke(Modifiers::default(), "f12"), HOME),
            ChordVerdict::Free(_)
        ));
    }

    #[test]
    fn verdict_names_the_shortcut_already_holding_the_chord() {
        let overrides = ShortcutOverrides::default();

        let verdict = overrides.verdict(&ctrl("2"), HOME);

        match verdict {
            ChordVerdict::Taken { owner_id, chord } => {
                assert_eq!(owner_id, CHAT);
                assert_eq!(chord, CHAT_DEFAULT);
            }
            _ => panic!("ctrl-2 is nav.chat's default and must read as taken"),
        }
    }

    #[test]
    fn verdict_lets_a_shortcut_recapture_the_chord_it_already_owns() {
        let overrides = ShortcutOverrides::default();

        assert!(
            matches!(overrides.verdict(&ctrl("2"), CHAT), ChordVerdict::Free(c) if c == CHAT_DEFAULT),
            "recapturing its own chord must not report the shortcut as its own conflict"
        );
    }

    #[test]
    fn a_snapshot_round_trips_through_the_stored_json_form() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());
        overrides.unbind(CHAT);
        let before: Vec<Option<String>> = SHORTCUTS
            .iter()
            .map(|entry| overrides.chord_of(entry).map(str::to_owned))
            .collect();

        let stored = serde_json::to_string(&overrides.snapshot()).unwrap();
        let mut restored = ShortcutOverrides::default();
        restored.replace_stored(Some(&stored));

        let after: Vec<Option<String>> = SHORTCUTS
            .iter()
            .map(|entry| restored.chord_of(entry).map(str::to_owned))
            .collect();
        assert_eq!(after, before);
    }

    #[test]
    fn replace_stored_with_no_stored_value_returns_every_shortcut_to_its_default() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());

        overrides.replace_stored(None);

        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
        assert!(!overrides.is_overridden(HOME));
    }
}
