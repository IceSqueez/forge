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
}
