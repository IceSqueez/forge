#![cfg(target_os = "linux")]

use crate::combo::HotkeyCombo;

const SEPARATOR: char = '+';

pub(crate) fn preferred_trigger(combo: &HotkeyCombo) -> Option<String> {
    let mut parts = combo.as_str().split(SEPARATOR);
    let key = keysym_name(parts.next_back()?)?;
    let mut tokens = parts
        .map(|m| modifier_token(m).map(str::to_owned))
        .collect::<Option<Vec<String>>>()?;
    tokens.push(key);
    Some(tokens.join("+"))
}

fn modifier_token(modifier: &str) -> Option<&'static str> {
    match modifier {
        "Ctrl" => Some("CTRL"),
        "Shift" => Some("SHIFT"),
        "Alt" => Some("ALT"),
        "Meta" => Some("LOGO"),
        _ => None,
    }
}

fn keysym_name(key: &str) -> Option<String> {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && c.is_ascii_alphanumeric()
    {
        return Some(c.to_ascii_lowercase().to_string());
    }

    let name = match key {
        "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12" => key,
        "Delete" | "Insert" | "Home" | "End" | "Tab" | "Escape" => key,
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "Backspace" => "BackSpace",
        "Enter" => "Return",
        "Space" => "space",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "Num0" => "KP_0",
        "Num1" => "KP_1",
        "Num2" => "KP_2",
        "Num3" => "KP_3",
        "Num4" => "KP_4",
        "Num5" => "KP_5",
        "Num6" => "KP_6",
        "Num7" => "KP_7",
        "Num8" => "KP_8",
        "Num9" => "KP_9",
        _ => return None,
    };
    Some(name.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn trigger(combo: &str) -> Option<String> {
        preferred_trigger(&HotkeyCombo::parse(combo).unwrap())
    }

    fn unchecked_combo(raw: &str) -> HotkeyCombo {
        serde_json::from_value(serde_json::Value::String(raw.to_owned())).unwrap()
    }

    #[test]
    fn each_key_class_converts_to_its_shortcuts_spec_keysym() {
        for (combo, expected) in [
            ("A", "a"),
            ("Ctrl+Z", "CTRL+z"),
            ("Shift+7", "SHIFT+7"),
            ("F12", "F12"),
            ("Ctrl+Enter", "CTRL+Return"),
            ("Alt+Space", "ALT+space"),
            ("Backspace", "BackSpace"),
            ("PageUp", "Page_Up"),
            ("PageDown", "Page_Down"),
            ("ArrowLeft", "Left"),
            ("Num5", "KP_5"),
            ("Escape", "Escape"),
            ("Meta+Delete", "LOGO+Delete"),
            ("Ctrl+Shift+Alt+Meta+Tab", "CTRL+SHIFT+ALT+LOGO+Tab"),
        ] {
            assert_eq!(
                trigger(combo).as_deref(),
                Some(expected),
                "wrong trigger for {combo}"
            );
        }
    }

    #[test]
    fn every_key_the_combo_parser_accepts_has_a_preferred_trigger() {
        let keys = ('A'..='Z')
            .chain('0'..='9')
            .map(String::from)
            .chain((1..=12).map(|n| format!("F{n}")))
            .chain((0..=9).map(|n| format!("Num{n}")))
            .chain(
                [
                    "Delete",
                    "Insert",
                    "Home",
                    "End",
                    "PageUp",
                    "PageDown",
                    "Backspace",
                    "Tab",
                    "Enter",
                    "Escape",
                    "Space",
                    "ArrowUp",
                    "ArrowDown",
                    "ArrowLeft",
                    "ArrowRight",
                ]
                .map(String::from),
            );
        for key in keys {
            assert!(trigger(&key).is_some(), "no preferred trigger for {key}");
        }
    }

    #[test]
    fn a_stored_combo_with_an_unknown_key_or_modifier_gets_no_trigger() {
        for raw in [
            "Ctrl+PrintScreen",
            "Hyper+A",
            "Ctrl+F13",
            "Ctrl+\u{e9}",
            "Ctrl+",
            "",
        ] {
            assert_eq!(
                preferred_trigger(&unchecked_combo(raw)),
                None,
                "expected no trigger for {raw:?}"
            );
        }
    }
}
