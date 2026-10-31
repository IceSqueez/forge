use std::collections::HashMap;

use crate::backend::HotkeyId;
use crate::combo::HotkeyCombo;

#[derive(Default)]
pub(crate) struct Registrations {
    by_id: HashMap<HotkeyId, HotkeyCombo>,
    by_combo: HashMap<String, HotkeyId>,
}

impl Registrations {
    pub(crate) fn contains_combo(&self, combo: &str) -> bool {
        self.by_combo.contains_key(combo)
    }

    pub(crate) fn combo_of(&self, id: HotkeyId) -> Option<&HotkeyCombo> {
        self.by_id.get(&id)
    }

    pub(crate) fn insert(&mut self, id: HotkeyId, combo: HotkeyCombo) {
        self.by_combo.insert(combo.as_str().to_owned(), id);
        self.by_id.insert(id, combo);
    }

    pub(crate) fn remove(&mut self, id: HotkeyId) -> Option<HotkeyCombo> {
        let combo = self.by_id.remove(&id)?;
        self.by_combo.remove(combo.as_str());
        Some(combo)
    }

    pub(crate) fn len(&self) -> usize {
        self.by_id.len()
    }

    pub(crate) fn entries(&self) -> Vec<(HotkeyId, HotkeyCombo)> {
        self.by_id
            .iter()
            .map(|(&id, combo)| (id, combo.clone()))
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn combo(text: &str) -> HotkeyCombo {
        HotkeyCombo::parse(text).expect("a valid combo")
    }

    #[test]
    fn insert_and_remove_keep_both_indexes_and_the_count_in_step() {
        let mut registrations = Registrations::default();
        registrations.insert(HotkeyId(1), combo("Ctrl+A"));
        registrations.insert(HotkeyId(2), combo("Ctrl+B"));

        assert_eq!(registrations.len(), 2);
        assert!(registrations.contains_combo("Ctrl+A"));

        let removed = registrations.remove(HotkeyId(1));

        assert_eq!(
            removed.map(|c| c.as_str().to_owned()),
            Some("Ctrl+A".to_owned())
        );
        assert_eq!(registrations.len(), 1);
        assert!(!registrations.contains_combo("Ctrl+A"));
        assert!(registrations.combo_of(HotkeyId(1)).is_none());
    }

    #[test]
    fn removing_an_unknown_id_changes_nothing() {
        let mut registrations = Registrations::default();
        registrations.insert(HotkeyId(1), combo("Ctrl+A"));

        assert!(registrations.remove(HotkeyId(9)).is_none());
        assert_eq!(registrations.len(), 1);
    }
}
