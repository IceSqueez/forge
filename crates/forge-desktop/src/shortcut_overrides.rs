use std::collections::HashMap;
use std::sync::Arc;

use forge_storage::SettingDisclosure::Kind;
use forge_storage::SettingPortability::Portable;
use forge_storage::{
    Setting, SettingCodec, SettingEncoding, SettingsRepo, StorageError, read_setting, write_setting,
};
use gpui::{App, Keystroke};
use serde::{Deserialize, Serialize};

use crate::actions::{
    SHORTCUTS, ShortcutEntry, canonical_chord, chord_is_bindable, effective_chord,
    reapply_key_bindings, shortcut_entry,
};
use crate::repo_failure::RepoFailure;

pub enum ChordVerdict {
    Unusable,
    NeedsModifier,
    Taken {
        owner_id: &'static str,
        chord: String,
    },
    Free(String),
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OverrideEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chord: Option<String>,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

impl Default for OverrideEntry {
    fn default() -> Self {
        Self {
            chord: None,
            enabled: true,
        }
    }
}

impl OverrideEntry {
    fn is_default(&self) -> bool {
        self.chord.is_none() && self.enabled
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredOverrides {
    Entries(HashMap<String, OverrideEntry>),
    ChordsOnly(HashMap<String, String>),
}

impl StoredOverrides {
    fn into_known_entries(self) -> HashMap<String, OverrideEntry> {
        let entries: HashMap<String, OverrideEntry> = match self {
            Self::Entries(map) => map,
            Self::ChordsOnly(chords) => chords
                .into_iter()
                .map(|(id, chord)| {
                    (
                        id,
                        OverrideEntry {
                            chord: Some(chord),
                            enabled: true,
                        },
                    )
                })
                .collect(),
        };
        entries
            .into_iter()
            .filter(|(id, _)| SHORTCUTS.iter().any(|entry| entry.id == id))
            .collect()
    }
}

#[derive(Default)]
pub struct StoredShortcuts {
    entries: HashMap<String, OverrideEntry>,
    unreadable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutsCodec;

impl SettingCodec for ShortcutsCodec {
    type Value = StoredShortcuts;
    const ENCODING: SettingEncoding = SettingEncoding::Json;

    fn default_value(&self) -> StoredShortcuts {
        StoredShortcuts::default()
    }

    fn decode(&self, raw: &str) -> Option<StoredShortcuts> {
        Some(match serde_json::from_str::<StoredOverrides>(raw) {
            Ok(stored) => StoredShortcuts {
                entries: stored.into_known_entries(),
                unreadable: false,
            },
            Err(_) => StoredShortcuts {
                entries: HashMap::new(),
                unreadable: true,
            },
        })
    }

    fn encode(&self, value: &StoredShortcuts) -> Option<String> {
        serde_json::to_string(&value.entries).ok()
    }
}

pub const KEYBOARD_SHORTCUTS: Setting<ShortcutsCodec> =
    Setting::new("app.keyboard_shortcuts", ShortcutsCodec, Portable, Kind);

pub async fn load_overrides(
    repo: Arc<dyn SettingsRepo>,
) -> Result<ShortcutOverrides, StorageError> {
    let stored = read_setting(repo.as_ref(), &KEYBOARD_SHORTCUTS).await?;
    if stored.unreadable {
        return Err(StorageError::UndecodableSetting {
            key: KEYBOARD_SHORTCUTS.name().to_owned(),
        });
    }
    Ok(ShortcutOverrides {
        map: stored.entries,
    })
}

#[derive(Default)]
pub struct ShortcutOverrides {
    map: HashMap<String, OverrideEntry>,
}

impl ShortcutOverrides {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn chord_of(&self, entry: &'static ShortcutEntry) -> Option<&str> {
        match self
            .map
            .get(entry.id)
            .and_then(|stored| stored.chord.as_deref())
        {
            Some(chord) if chord_is_bindable(chord) => Some(chord),
            Some(_) => None,
            None => Some(entry.default_chord),
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.map.get(id).is_none_or(|stored| stored.enabled)
    }

    pub fn is_overridden(&self, id: &str) -> bool {
        self.map
            .get(id)
            .is_some_and(|stored| stored.chord.is_some())
    }

    fn keymap_overrides(&self) -> HashMap<String, String> {
        SHORTCUTS
            .iter()
            .filter_map(|entry| {
                if !self.is_enabled(entry.id) {
                    return Some((entry.id.to_owned(), String::new()));
                }
                let chord = self.map.get(entry.id)?.chord.clone()?;
                Some((entry.id.to_owned(), chord))
            })
            .collect()
    }

    pub fn bound_count(&self) -> usize {
        let keymap = self.keymap_overrides();
        SHORTCUTS
            .iter()
            .filter(|entry| effective_chord(&keymap, entry).is_some())
            .count()
    }

    pub fn owner_of(&self, chord: &str, exclude: &str) -> Option<&'static str> {
        SHORTCUTS
            .iter()
            .find(|entry| entry.id != exclude && self.chord_of(entry) == Some(chord))
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

    fn entry_mut(&mut self, id: &str) -> &mut OverrideEntry {
        self.map.entry(id.to_owned()).or_default()
    }

    fn prune(&mut self, id: &str) {
        if self.map.get(id).is_some_and(OverrideEntry::is_default) {
            self.map.remove(id);
        }
    }

    pub fn bind(&mut self, id: &str, chord: String) {
        let default = shortcut_entry(id).map(|entry| entry.default_chord);
        self.entry_mut(id).chord = if default == Some(chord.as_str()) {
            None
        } else {
            Some(chord)
        };
        self.prune(id);
    }

    pub fn unbind(&mut self, id: &str) {
        self.entry_mut(id).chord = Some(String::new());
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        self.entry_mut(id).enabled = enabled;
        self.prune(id);
    }

    pub fn reset(&mut self, id: &str) {
        self.entry_mut(id).chord = None;
        self.prune(id);
    }

    pub fn reset_all(&mut self) {
        self.map.clear();
    }

    pub fn apply(&self, cx: &mut App) {
        reapply_key_bindings(cx, &self.keymap_overrides());
    }

    pub fn snapshot(&self) -> HashMap<String, OverrideEntry> {
        self.map.clone()
    }
}

pub async fn save_overrides(
    repo: Arc<dyn SettingsRepo>,
    map: HashMap<String, OverrideEntry>,
) -> Result<(), RepoFailure> {
    let stored = StoredShortcuts {
        entries: map,
        unreadable: false,
    };
    write_setting(repo.as_ref(), &KEYBOARD_SHORTCUTS, &stored)
        .await
        .map_err(RepoFailure::from)
}

#[cfg(test)]
mod tests {
    use gpui::Modifiers;

    use super::*;
    use crate::test_support::stored_shortcut_overrides;
    use forge_storage::RowClasses;
    use forge_storage::test_support::MemorySettings;

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
        let restored = stored_shortcut_overrides(Some(&stored)).unwrap();

        let after: Vec<Option<String>> = SHORTCUTS
            .iter()
            .map(|entry| restored.chord_of(entry).map(str::to_owned))
            .collect();
        assert_eq!(after, before);
    }

    fn round_trip(overrides: &ShortcutOverrides) -> ShortcutOverrides {
        let stored = serde_json::to_string(&overrides.snapshot()).unwrap();
        stored_shortcut_overrides(Some(&stored)).unwrap()
    }

    #[test]
    fn load_overrides_decodes_the_current_form_and_reads_older_forms_as_enabled() {
        let cases = [
            (
                "current form",
                Some(r#"{"nav.home":{"chord":"ctrl-9","enabled":false},"nav.chat":{"chord":""}}"#),
                Some("ctrl-9"),
                false,
                None,
            ),
            (
                "flat map",
                Some(r#"{"nav.home":"ctrl-9","nav.chat":""}"#),
                Some("ctrl-9"),
                true,
                None,
            ),
            (
                "object map without the toggle",
                Some(r#"{"nav.home":{"chord":"ctrl-9"},"nav.chat":{"chord":""}}"#),
                Some("ctrl-9"),
                true,
                None,
            ),
            (
                "nothing stored",
                None,
                Some(HOME_DEFAULT),
                true,
                Some(CHAT_DEFAULT),
            ),
        ];

        for (case, raw, home_chord, home_enabled, chat_chord) in cases {
            let overrides = stored_shortcut_overrides(raw).unwrap();

            assert_eq!(
                (
                    chord(&overrides, HOME).as_deref(),
                    overrides.is_enabled(HOME),
                    chord(&overrides, CHAT).as_deref(),
                    overrides.is_enabled(CHAT),
                ),
                (home_chord, home_enabled, chat_chord, true),
                "{case}"
            );
        }
    }

    #[test]
    fn load_overrides_drops_ids_the_roster_no_longer_defines_in_either_form() {
        for raw in [
            r#"{"nav.home":"ctrl-9","nav.gone":"ctrl-8"}"#,
            r#"{"nav.home":{"chord":"ctrl-9"},"nav.gone":{"chord":"ctrl-8"}}"#,
        ] {
            let overrides = stored_shortcut_overrides(Some(raw)).unwrap();

            assert_eq!(
                (
                    chord(&overrides, HOME).as_deref(),
                    overrides.map.contains_key("nav.gone"),
                ),
                (Some("ctrl-9"), false),
                "{raw}"
            );
        }
    }

    #[test]
    fn load_overrides_passes_a_store_failure_through() {
        let (backend, _writes) = crate::test_support::test_backend();
        backend.fault_reads(
            KEYBOARD_SHORTCUTS.name(),
            crate::test_support::ReadFault::Fails,
        );

        let loaded = crate::test_support::runtime().block_on(load_overrides(backend));

        assert!(matches!(loaded, Err(StorageError::Connection { .. })));
    }

    #[test]
    fn load_overrides_rejects_a_value_neither_stored_form_decodes_naming_the_key() {
        for raw in [
            "",
            "not json",
            "[]",
            r#"{"nav.home":7}"#,
            r#"{"nav.home":{"chord":7}}"#,
            r#"{"nav.home":"ctrl-9","nav.chat":{"chord":"ctrl-8"}}"#,
        ] {
            assert!(
                matches!(
                    stored_shortcut_overrides(Some(raw)),
                    Err(StorageError::UndecodableSetting { key }) if key == KEYBOARD_SHORTCUTS.name()
                ),
                "expected an undecodable-setting error for {raw:?}"
            );
        }
    }

    #[test]
    fn the_codec_flags_a_corrupt_value_unreadable_instead_of_rejecting_it() {
        for raw in ["", "not json", "[]", r#"{"nav.home":7}"#] {
            let decoded = ShortcutsCodec
                .decode(raw)
                .expect("a rejected value would read as an empty map");

            assert!(
                decoded.unreadable && decoded.entries.is_empty(),
                "{raw:?} decoded as readable"
            );
        }
    }

    #[test]
    fn the_codec_reads_a_stored_map_as_readable() {
        let decoded = ShortcutsCodec
            .decode(r#"{"nav.home":"ctrl-9"}"#)
            .expect("a well-formed map decodes");

        assert!(!decoded.unreadable && decoded.entries.contains_key(HOME));
    }

    #[test]
    fn a_failed_load_of_a_corrupt_value_leaves_the_stored_row_untouched() {
        let memory = Arc::new(MemorySettings::new());
        memory.seed_raw(&KEYBOARD_SHORTCUTS, "not json");

        let loaded = crate::test_support::runtime().block_on(load_overrides(memory.clone()));

        assert!(matches!(
            loaded,
            Err(StorageError::UndecodableSetting { .. })
        ));
        assert_eq!(
            (
                memory.write_log(),
                memory.row(KEYBOARD_SHORTCUTS.name()).map(|row| row.value)
            ),
            (Vec::new(), Some("not json".to_owned()))
        );
    }

    #[test]
    fn saving_overrides_stamps_the_declared_row_classes() {
        let memory = Arc::new(MemorySettings::new());
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());

        crate::test_support::runtime()
            .block_on(save_overrides(memory.clone(), overrides.map))
            .unwrap();

        assert_eq!(
            memory.row(KEYBOARD_SHORTCUTS.name()).map(|row| row.classes),
            Some(RowClasses {
                portability: Portable,
                disclosure: Kind,
            })
        );
    }

    #[test]
    fn a_disabled_shortcut_survives_the_save_and_parse_round_trip() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());
        overrides.set_enabled(HOME, false);
        overrides.set_enabled(CHAT, false);

        let restored = round_trip(&overrides);

        assert_eq!(chord(&restored, HOME).as_deref(), Some("ctrl-9"));
        assert!(!restored.is_enabled(HOME));
        assert_eq!(
            chord(&restored, CHAT).as_deref(),
            Some(CHAT_DEFAULT),
            "a shortcut disabled without a rebind keeps its default chord"
        );
        assert!(!restored.is_enabled(CHAT));
    }

    #[test]
    fn an_entry_is_pruned_once_it_reduces_to_the_roster_default_and_enabled() {
        let mut overrides = ShortcutOverrides::default();

        overrides.set_enabled(HOME, false);
        assert!(!overrides.is_empty(), "a disabled default must be stored");

        overrides.set_enabled(HOME, true);
        assert!(
            overrides.is_empty(),
            "an entry carrying no chord and no disable is dead weight"
        );
    }

    #[test]
    fn reset_restores_the_default_chord_but_leaves_a_disabled_shortcut_disabled() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());
        overrides.set_enabled(HOME, false);

        overrides.reset(HOME);

        assert_eq!(chord(&overrides, HOME).as_deref(), Some(HOME_DEFAULT));
        assert!(!overrides.is_enabled(HOME));
        assert!(!overrides.is_empty());
    }

    #[test]
    fn disabling_a_shortcut_hides_its_chord_from_the_keymap_without_forgetting_it() {
        let mut overrides = ShortcutOverrides::default();
        overrides.bind(HOME, "ctrl-9".to_owned());

        overrides.set_enabled(HOME, false);
        assert_eq!(
            overrides.keymap_overrides().get(HOME).map(String::as_str),
            Some(""),
            "a disabled shortcut must reach the keymap as an unbindable chord"
        );
        assert_eq!(
            chord(&overrides, HOME).as_deref(),
            Some("ctrl-9"),
            "the row must still show the chord it will get back"
        );

        overrides.set_enabled(HOME, true);
        assert_eq!(
            overrides.keymap_overrides().get(HOME).map(String::as_str),
            Some("ctrl-9")
        );
    }

    #[test]
    fn bound_count_counts_only_shortcuts_that_have_a_chord_and_are_enabled() {
        let mut overrides = ShortcutOverrides::default();
        assert_eq!(overrides.bound_count(), SHORTCUTS.len());

        overrides.unbind(HOME);
        assert_eq!(overrides.bound_count(), SHORTCUTS.len() - 1);

        overrides.set_enabled(CHAT, false);
        assert_eq!(overrides.bound_count(), SHORTCUTS.len() - 2);

        overrides.set_enabled(CHAT, true);
        assert_eq!(overrides.bound_count(), SHORTCUTS.len() - 1);
    }

    #[test]
    fn a_disabled_shortcut_still_owns_its_chord_for_conflict_detection() {
        let mut overrides = ShortcutOverrides::default();
        overrides.set_enabled(CHAT, false);

        assert_eq!(
            overrides.owner_of(CHAT_DEFAULT, HOME),
            Some(CHAT),
            "disabling must not silently free the chord for another shortcut to shadow"
        );
    }

    #[test]
    fn an_id_outside_the_roster_never_reaches_a_real_shortcut_or_a_reload() {
        let mut overrides = ShortcutOverrides::default();

        overrides.set_enabled("nav.gone", false);
        overrides.bind("nav.gone", "ctrl-9".to_owned());

        assert!(overrides.is_enabled(HOME));
        assert_eq!(overrides.bound_count(), SHORTCUTS.len());
        assert!(overrides.keymap_overrides().is_empty());
        assert!(
            round_trip(&overrides).is_empty(),
            "an unknown id must not survive a reload"
        );
    }
}
