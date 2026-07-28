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
