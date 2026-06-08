use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::HotkeyError;

const MOD_ORDER: &[&str] = &["Ctrl", "Shift", "Alt", "Meta"];

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
    fn parse_simple_letter() {
        let c = HotkeyCombo::parse("A").unwrap();
        assert_eq!(c.as_str(), "A");
    }

    #[test]
    fn parse_ctrl_shift_a() {
        let c = HotkeyCombo::parse("Ctrl+Shift+A").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Shift+A");
    }

    #[test]
    fn parse_lowercase_normalizes_to_canonical() {
        let c = HotkeyCombo::parse("ctrl+shift+a").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Shift+A");
    }

    #[test]
    fn parse_uppercase_input() {
        let c = HotkeyCombo::parse("CTRL+SHIFT+A").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Shift+A");
    }

    #[test]
    fn parse_reorders_modifiers_to_canonical_order() {
        let c = HotkeyCombo::parse("Shift+Ctrl+A").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Shift+A");
    }

    #[test]
    fn parse_alt_meta_order() {
        let c = HotkeyCombo::parse("Meta+Alt+F1").unwrap();
        assert_eq!(c.as_str(), "Alt+Meta+F1");
    }

    #[test]
    fn parse_control_alias() {
        let c = HotkeyCombo::parse("Control+A").unwrap();
        assert_eq!(c.as_str(), "Ctrl+A");
    }

    #[test]
    fn parse_option_alias() {
        let c = HotkeyCombo::parse("Option+A").unwrap();
        assert_eq!(c.as_str(), "Alt+A");
    }

    #[test]
    fn parse_cmd_alias() {
        let c = HotkeyCombo::parse("Cmd+A").unwrap();
        assert_eq!(c.as_str(), "Meta+A");
    }

    #[test]
    fn parse_super_alias() {
        let c = HotkeyCombo::parse("Super+A").unwrap();
        assert_eq!(c.as_str(), "Meta+A");
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
    fn parse_function_keys() {
        for n in 1..=12u8 {
            let input = format!("Ctrl+F{n}");
            let expected = format!("Ctrl+F{n}");
            let c = HotkeyCombo::parse(&input).unwrap();
            assert_eq!(c.as_str(), expected, "F{n} failed");
        }
    }

    #[test]
    fn parse_digits() {
        for d in 0..=9u8 {
            let input = format!("Ctrl+{d}");
            let c = HotkeyCombo::parse(&input).unwrap();
            assert_eq!(c.as_str(), format!("Ctrl+{d}"));
        }
    }

    #[test]
    fn parse_named_keys() {
        let cases = [
            ("Ctrl+Delete", "Ctrl+Delete"),
            ("Ctrl+Insert", "Ctrl+Insert"),
            ("Ctrl+Home", "Ctrl+Home"),
            ("Ctrl+End", "Ctrl+End"),
            ("Ctrl+PageUp", "Ctrl+PageUp"),
            ("Ctrl+PageDown", "Ctrl+PageDown"),
            ("Ctrl+Backspace", "Ctrl+Backspace"),
            ("Ctrl+Tab", "Ctrl+Tab"),
            ("Ctrl+Enter", "Ctrl+Enter"),
            ("Ctrl+Escape", "Ctrl+Escape"),
            ("Ctrl+Space", "Ctrl+Space"),
        ];
        for (input, expected) in &cases {
            let c = HotkeyCombo::parse(input).unwrap();
            assert_eq!(c.as_str(), *expected, "failed: {input}");
        }
    }

    #[test]
    fn parse_arrow_keys() {
        let cases = [
            ("Ctrl+Up", "Ctrl+ArrowUp"),
            ("Ctrl+Down", "Ctrl+ArrowDown"),
            ("Ctrl+Left", "Ctrl+ArrowLeft"),
            ("Ctrl+Right", "Ctrl+ArrowRight"),
            ("Ctrl+ArrowUp", "Ctrl+ArrowUp"),
        ];
        for (input, expected) in &cases {
            let c = HotkeyCombo::parse(input).unwrap();
            assert_eq!(c.as_str(), *expected, "failed: {input}");
        }
    }

    #[test]
    fn parse_numpad_keys() {
        let c = HotkeyCombo::parse("Ctrl+Num5").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Num5");
        let c = HotkeyCombo::parse("Ctrl+Numpad5").unwrap();
        assert_eq!(c.as_str(), "Ctrl+Num5");
    }

    #[test]
    fn parse_empty_string_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse(""),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn parse_just_plus_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse("+"),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn parse_trailing_plus_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse("Ctrl+"),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn parse_only_modifier_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse("Ctrl+Shift"),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn parse_unknown_token_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse("Ctrl+XYZ123"),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn parse_two_key_codes_is_invalid() {
        assert!(matches!(
            HotkeyCombo::parse("A+B"),
            Err(HotkeyError::InvalidCombo(_))
        ));
    }

    #[test]
    fn display_matches_as_str() {
        let c = HotkeyCombo::parse("Ctrl+Shift+A").unwrap();
        assert_eq!(c.to_string(), c.as_str());
    }

    #[test]
    fn serde_roundtrip() {
        let c = HotkeyCombo::parse("Ctrl+Shift+F5").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        let back: HotkeyCombo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn duplicate_modifiers_deduplicated() {
        let c = HotkeyCombo::parse("Ctrl+Ctrl+A").unwrap();
        assert_eq!(c.as_str(), "Ctrl+A");
    }
}
