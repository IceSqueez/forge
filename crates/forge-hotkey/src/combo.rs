use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::HotkeyError;

const MOD_ORDER: &[&str] = &["Ctrl", "Shift", "Alt", "Meta"];

const GRAB_CONSUMES_KEYSTROKE: bool = cfg!(any(target_os = "windows", target_os = "macos"));

const TYPING_KEYS: &[&str] = &[
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "Num0",
    "Num1",
    "Num2",
    "Num3",
    "Num4",
    "Num5",
    "Num6",
    "Num7",
    "Num8",
    "Num9",
    "Space",
    "Enter",
    "Tab",
    "Backspace",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HotkeyCombo(String);

impl HotkeyCombo {
    pub fn parse(s: &str) -> Result<Self, HotkeyError> {
        if s.is_empty() {
            return Err(HotkeyError::InvalidCombo(s.to_owned()));
        }

        let tokens: Vec<&str> = s.split('+').collect();
        let mut modifiers: Vec<&'static str> = Vec::new();
        let mut key: Option<String> = None;

        for token in &tokens {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                return Err(HotkeyError::InvalidCombo(s.to_owned()));
            }
            if let Some(m) = normalize_modifier(trimmed) {
                if !modifiers.contains(&m) {
                    modifiers.push(m);
                }
            } else if let Some(k) = normalize_key(trimmed) {
                if key.is_some() {
                    return Err(HotkeyError::InvalidCombo(s.to_owned()));
                }
                key = Some(k);
            } else {
                return Err(HotkeyError::InvalidCombo(s.to_owned()));
            }
        }

        let key = key.ok_or_else(|| HotkeyError::InvalidCombo(s.to_owned()))?;

        modifiers.sort_by_key(|m| {
            MOD_ORDER
                .iter()
                .position(|k| k == m)
                .unwrap_or(MOD_ORDER.len())
        });

        let mut parts: Vec<String> = modifiers.iter().map(|&m| m.to_owned()).collect();
        parts.push(key);
        Ok(Self(parts.join("+")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True where the OS grab consumes the keystroke system-wide (macOS, Windows) and the combo is
    /// a modifier-less key used for typing, so binding it stops that key from reaching other apps.
    pub fn swallows_typing(&self) -> bool {
        GRAB_CONSUMES_KEYSTROKE && !self.0.contains('+') && TYPING_KEYS.contains(&self.0.as_str())
    }
}

impl fmt::Display for HotkeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn normalize_modifier(s: &str) -> Option<&'static str> {
    match s.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some("Ctrl"),
        "shift" => Some("Shift"),
        "alt" | "option" => Some("Alt"),
        "meta" | "cmd" | "super" | "win" => Some("Meta"),
        "cmdorctrl" | "commandorcontrol" => {
            #[cfg(target_os = "macos")]
            {
                Some("Meta")
            }
            #[cfg(not(target_os = "macos"))]
            {
                Some("Ctrl")
            }
        }
        _ => None,
    }
}

fn normalize_key(s: &str) -> Option<String> {
    match s.to_ascii_lowercase().as_str() {
        "a" | "b" | "c" | "d" | "e" | "f" | "g" | "h" | "i" | "j" | "k" | "l" | "m" | "n" | "o"
        | "p" | "q" | "r" | "s" | "t" | "u" | "v" | "w" | "x" | "y" | "z" => {
            Some(s.to_ascii_uppercase())
        }
        "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => Some(s.to_owned()),
        "f1" => Some("F1".to_owned()),
        "f2" => Some("F2".to_owned()),
        "f3" => Some("F3".to_owned()),
        "f4" => Some("F4".to_owned()),
        "f5" => Some("F5".to_owned()),
        "f6" => Some("F6".to_owned()),
        "f7" => Some("F7".to_owned()),
        "f8" => Some("F8".to_owned()),
        "f9" => Some("F9".to_owned()),
        "f10" => Some("F10".to_owned()),
        "f11" => Some("F11".to_owned()),
        "f12" => Some("F12".to_owned()),
        "delete" | "del" => Some("Delete".to_owned()),
        "insert" | "ins" => Some("Insert".to_owned()),
        "home" => Some("Home".to_owned()),
        "end" => Some("End".to_owned()),
        "pageup" | "pgup" => Some("PageUp".to_owned()),
        "pagedown" | "pgdn" | "pgdown" => Some("PageDown".to_owned()),
        "backspace" => Some("Backspace".to_owned()),
        "tab" => Some("Tab".to_owned()),
        "enter" | "return" => Some("Enter".to_owned()),
        "escape" | "esc" => Some("Escape".to_owned()),
        "space" => Some("Space".to_owned()),
        "arrowup" | "up" => Some("ArrowUp".to_owned()),
        "arrowdown" | "down" => Some("ArrowDown".to_owned()),
        "arrowleft" | "left" => Some("ArrowLeft".to_owned()),
        "arrowright" | "right" => Some("ArrowRight".to_owned()),
        "num0" | "numpad0" => Some("Num0".to_owned()),
        "num1" | "numpad1" => Some("Num1".to_owned()),
        "num2" | "numpad2" => Some("Num2".to_owned()),
        "num3" | "numpad3" => Some("Num3".to_owned()),
        "num4" | "numpad4" => Some("Num4".to_owned()),
        "num5" | "numpad5" => Some("Num5".to_owned()),
        "num6" | "numpad6" => Some("Num6".to_owned()),
        "num7" | "numpad7" => Some("Num7".to_owned()),
        "num8" | "numpad8" => Some("Num8".to_owned()),
        "num9" | "numpad9" => Some("Num9".to_owned()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            // Why: the Linux portal and evdev backends observe keys without consuming them, so a
            // bare key never stops typing there; the Carbon and RegisterHotKey grabs do.
            let expected = typing_key && cfg!(any(target_os = "windows", target_os = "macos"));
            assert_eq!(
                HotkeyCombo::parse(input).unwrap().swallows_typing(),
                expected,
                "wrong verdict for {input:?}"
            );
        }
    }
}
