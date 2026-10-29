use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::HotkeyError;
use crate::vocabulary::keys::{COMBO_SEPARATOR, Key, Modifier, Modifiers};

const GRAB_CONSUMES_KEYSTROKE: bool = cfg!(any(target_os = "windows", target_os = "macos"));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotkeyCombo {
    modifiers: Modifiers,
    key: Key,
    canonical: String,
}

impl HotkeyCombo {
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        let mut canonical = String::new();
        for name in modifiers.iter().map(Modifier::name) {
            canonical.push_str(name);
            canonical.push(COMBO_SEPARATOR);
        }
        canonical.push_str(key.name());
        Self {
            modifiers,
            key,
            canonical,
        }
    }

    pub fn parse(s: &str) -> Result<Self, HotkeyError> {
        let invalid = || HotkeyError::InvalidCombo(s.to_owned());
        if s.is_empty() {
            return Err(invalid());
        }

        let mut modifiers = Modifiers::NONE;
        let mut key: Option<Key> = None;

        for token in s.split(COMBO_SEPARATOR) {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                return Err(invalid());
            }
            if let Some(modifier) = Modifier::from_alias(trimmed) {
                modifiers = modifiers.with(modifier);
            } else if let Some(parsed) = Key::from_alias(trimmed) {
                if key.replace(parsed).is_some() {
                    return Err(invalid());
                }
            } else {
                return Err(invalid());
            }
        }

        key.map(|key| Self::new(modifiers, key)).ok_or_else(invalid)
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn key(&self) -> Key {
        self.key
    }

    pub fn swallows_typing(&self) -> bool {
        GRAB_CONSUMES_KEYSTROKE && self.modifiers.is_empty() && self.key.types_text()
    }
}

impl fmt::Display for HotkeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl Serialize for HotkeyCombo {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical)
    }
}

impl<'de> Deserialize<'de> for HotkeyCombo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonicalises_case_aliases_and_modifier_order() {
        for (input, canonical) in [
            ("a", "A"),
            ("ctrl+shift+a", "Ctrl+Shift+A"),
            ("CTRL+SHIFT+A", "Ctrl+Shift+A"),
            ("Shift+Ctrl+A", "Ctrl+Shift+A"),
            ("Meta+Alt+F1", "Alt+Meta+F1"),
            ("Ctrl+Ctrl+A", "Ctrl+A"),
            ("Control+A", "Ctrl+A"),
            ("Option+A", "Alt+A"),
            ("Cmd+A", "Meta+A"),
            ("Super+A", "Meta+A"),
            ("Win+A", "Meta+A"),
            ("ctrl+del", "Ctrl+Delete"),
            ("ctrl+ins", "Ctrl+Insert"),
            ("ctrl+home", "Ctrl+Home"),
            ("ctrl+end", "Ctrl+End"),
            ("ctrl+pgup", "Ctrl+PageUp"),
            ("ctrl+pgdn", "Ctrl+PageDown"),
            ("ctrl+backspace", "Ctrl+Backspace"),
            ("ctrl+tab", "Ctrl+Tab"),
            ("ctrl+return", "Ctrl+Enter"),
            ("ctrl+esc", "Ctrl+Escape"),
            ("ctrl+space", "Ctrl+Space"),
            ("ctrl+up", "Ctrl+ArrowUp"),
            ("ctrl+down", "Ctrl+ArrowDown"),
            ("ctrl+left", "Ctrl+ArrowLeft"),
            ("ctrl+right", "Ctrl+ArrowRight"),
            ("ctrl+numpad5", "Ctrl+Num5"),
            (" Ctrl + F5 ", "Ctrl+F5"),
        ] {
            assert_eq!(
                HotkeyCombo::parse(input).unwrap().as_str(),
                canonical,
                "wrong canonical form for {input:?}"
            );
        }
    }

    #[test]
    fn parse_accepts_every_function_key_and_digit() {
        let function_keys = (1..=12u8).map(|n| (format!("ctrl+f{n}"), format!("Ctrl+F{n}")));
        let digits = (0..=9u8).map(|d| (format!("ctrl+{d}"), format!("Ctrl+{d}")));
        for (input, canonical) in function_keys.chain(digits) {
            assert_eq!(HotkeyCombo::parse(&input).unwrap().as_str(), canonical);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cmdorctrl_resolves_to_meta_on_macos() {
        let c = HotkeyCombo::parse("CmdOrCtrl+Shift+1").unwrap();
        assert_eq!(c.as_str(), "Shift+Meta+1");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn cmdorctrl_resolves_to_ctrl_on_non_macos() {
        let c = HotkeyCombo::parse("CmdOrCtrl+Shift+1").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Shift+1");
    }

    #[test]
    fn parse_rejects_malformed_combos_and_echoes_the_input() {
        for bad in [
            "",
            "+",
            "Ctrl+",
            "Ctrl++A",
            "Ctrl+Shift",
            "Ctrl+XYZ123",
            "A+B",
            "F13",
        ] {
            assert!(
                matches!(HotkeyCombo::parse(bad), Err(HotkeyError::InvalidCombo(ref echoed)) if echoed == bad),
                "expected InvalidCombo({bad:?})"
            );
        }
    }

    #[test]
    fn only_modifier_less_typing_keys_swallow_typing_and_only_where_the_grab_consumes_the_key() {
        for (input, typing_key) in [
            ("A", true),
            ("1", true),
            ("Num1", true),
            ("Space", true),
            ("Enter", true),
            ("Tab", true),
            ("Backspace", true),
            ("F5", false),
            ("Escape", false),
            ("Delete", false),
            ("ArrowUp", false),
            ("Ctrl+A", false),
            ("Shift+1", false),
            ("Alt+Space", false),
        ] {
            let expected = typing_key && cfg!(any(target_os = "windows", target_os = "macos"));
            assert_eq!(
                HotkeyCombo::parse(input).unwrap().swallows_typing(),
                expected,
                "wrong verdict for {input:?}"
            );
        }
    }
    const STORED_KEY_TOKENS: &[(&str, &str)] = &[
        ("a", "A"),
        ("b", "B"),
        ("c", "C"),
        ("d", "D"),
        ("e", "E"),
        ("f", "F"),
        ("g", "G"),
        ("h", "H"),
        ("i", "I"),
        ("j", "J"),
        ("k", "K"),
        ("l", "L"),
        ("m", "M"),
        ("n", "N"),
        ("o", "O"),
        ("p", "P"),
        ("q", "Q"),
        ("r", "R"),
        ("s", "S"),
        ("t", "T"),
        ("u", "U"),
        ("v", "V"),
        ("w", "W"),
        ("x", "X"),
        ("y", "Y"),
        ("z", "Z"),
        ("0", "0"),
        ("1", "1"),
        ("2", "2"),
        ("3", "3"),
        ("4", "4"),
        ("5", "5"),
        ("6", "6"),
        ("7", "7"),
        ("8", "8"),
        ("9", "9"),
        ("f1", "F1"),
        ("f2", "F2"),
        ("f3", "F3"),
        ("f4", "F4"),
        ("f5", "F5"),
        ("f6", "F6"),
        ("f7", "F7"),
        ("f8", "F8"),
        ("f9", "F9"),
        ("f10", "F10"),
        ("f11", "F11"),
        ("f12", "F12"),
        ("delete", "Delete"),
        ("del", "Delete"),
        ("insert", "Insert"),
        ("ins", "Insert"),
        ("home", "Home"),
        ("end", "End"),
        ("pageup", "PageUp"),
        ("pgup", "PageUp"),
        ("pagedown", "PageDown"),
        ("pgdn", "PageDown"),
        ("pgdown", "PageDown"),
        ("backspace", "Backspace"),
        ("tab", "Tab"),
        ("enter", "Enter"),
        ("return", "Enter"),
        ("escape", "Escape"),
        ("esc", "Escape"),
        ("space", "Space"),
        ("arrowup", "ArrowUp"),
        ("up", "ArrowUp"),
        ("arrowdown", "ArrowDown"),
        ("down", "ArrowDown"),
        ("arrowleft", "ArrowLeft"),
        ("left", "ArrowLeft"),
        ("arrowright", "ArrowRight"),
        ("right", "ArrowRight"),
        ("num0", "Num0"),
        ("numpad0", "Num0"),
        ("num1", "Num1"),
        ("numpad1", "Num1"),
        ("num2", "Num2"),
        ("numpad2", "Num2"),
        ("num3", "Num3"),
        ("numpad3", "Num3"),
        ("num4", "Num4"),
        ("numpad4", "Num4"),
        ("num5", "Num5"),
        ("numpad5", "Num5"),
        ("num6", "Num6"),
        ("numpad6", "Num6"),
        ("num7", "Num7"),
        ("numpad7", "Num7"),
        ("num8", "Num8"),
        ("numpad8", "Num8"),
        ("num9", "Num9"),
        ("numpad9", "Num9"),
    ];

    fn primary_modifier() -> &'static str {
        if cfg!(target_os = "macos") {
            "Meta"
        } else {
            "Ctrl"
        }
    }

    #[test]
    fn every_key_token_stored_combos_may_hold_keeps_its_canonical_form() {
        for &(token, canonical) in STORED_KEY_TOKENS {
            for spelling in [token.to_owned(), token.to_ascii_uppercase()] {
                for (input, expected) in [
                    (spelling.clone(), canonical.to_owned()),
                    (
                        format!("shift+ctrl+{spelling}"),
                        format!("Ctrl+Shift+{canonical}"),
                    ),
                ] {
                    let parsed = HotkeyCombo::parse(&input)
                        .unwrap_or_else(|e| panic!("{input:?} no longer parses: {e}"));
                    assert_eq!(parsed.as_str(), expected, "for {input:?}");
                }
            }
        }
    }

    #[test]
    fn stored_key_tokens_cover_every_key() {
        for key in Key::ALL {
            assert!(
                STORED_KEY_TOKENS
                    .iter()
                    .any(|&(_, canonical)| canonical == key.name()),
                "{key:?} is accepted but missing from the stored-token table"
            );
        }
    }

    #[test]
    fn every_modifier_token_stored_combos_may_hold_keeps_its_canonical_form() {
        for (token, canonical) in [
            ("ctrl", "Ctrl"),
            ("control", "Ctrl"),
            ("shift", "Shift"),
            ("alt", "Alt"),
            ("option", "Alt"),
            ("meta", "Meta"),
            ("cmd", "Meta"),
            ("super", "Meta"),
            ("win", "Meta"),
            ("cmdorctrl", primary_modifier()),
            ("commandorcontrol", primary_modifier()),
        ] {
            for spelling in [token.to_owned(), token.to_ascii_uppercase()] {
                let input = format!("{spelling}+F5");
                assert_eq!(
                    HotkeyCombo::parse(&input).unwrap().as_str(),
                    format!("{canonical}+F5"),
                    "for {input:?}"
                );
            }
        }
    }

    #[test]
    fn every_key_name_resolves_back_to_its_key() {
        for key in Key::ALL {
            assert_eq!(Key::from_alias(key.name()), Some(key), "{key:?}");
            assert_eq!(
                Key::from_alias(&key.name().to_ascii_lowercase()),
                Some(key),
                "{key:?} lower-case"
            );
        }
    }

    #[test]
    fn every_canonical_combo_parses_back_to_itself() {
        let modifier_sets = (0..1u8 << Modifier::ALL.len()).map(|mask| {
            Modifier::ALL
                .into_iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, modifier)| modifier)
                .collect::<Modifiers>()
        });
        for modifiers in modifier_sets {
            for key in Key::ALL {
                let combo = HotkeyCombo::new(modifiers, key);
                assert_eq!(
                    HotkeyCombo::parse(combo.as_str()).ok(),
                    Some(combo.clone()),
                    "{}",
                    combo.as_str()
                );
            }
        }
    }

    #[test]
    fn deserialize_canonicalises_a_valid_string_and_serializes_it_back() {
        let combo: HotkeyCombo =
            serde_json::from_value(serde_json::json!("shift+ctrl+pgup")).unwrap();

        assert_eq!(
            serde_json::to_value(&combo).unwrap(),
            serde_json::json!("Ctrl+Shift+PageUp")
        );
    }

    #[test]
    fn deserialize_rejects_an_invalid_combo_or_a_non_string() {
        for bad in [
            serde_json::json!(""),
            serde_json::json!("Ctrl+"),
            serde_json::json!("Ctrl+PrintScreen"),
            serde_json::json!("Hyper+A"),
            serde_json::json!("A+B"),
            serde_json::json!(5),
            serde_json::json!(null),
            serde_json::json!(["Ctrl", "A"]),
        ] {
            assert!(
                serde_json::from_value::<HotkeyCombo>(bad.clone()).is_err(),
                "accepted {bad}"
            );
        }
    }
}
