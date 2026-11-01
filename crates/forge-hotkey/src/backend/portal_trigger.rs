#![cfg(target_os = "linux")]

use crate::combo::HotkeyCombo;
use crate::vocabulary::keys::{Key, Modifier};

const SEPARATOR: &str = "+";

pub(crate) fn preferred_trigger(combo: &HotkeyCombo) -> String {
    combo
        .modifiers()
        .iter()
        .map(modifier_token)
        .chain(std::iter::once(keysym_name(combo.key())))
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

fn modifier_token(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Ctrl => "CTRL",
        Modifier::Shift => "SHIFT",
        Modifier::Alt => "ALT",
        Modifier::Meta => "LOGO",
    }
}

fn keysym_name(key: Key) -> &'static str {
    match key {
        Key::A => "a",
        Key::B => "b",
        Key::C => "c",
        Key::D => "d",
        Key::E => "e",
        Key::F => "f",
        Key::G => "g",
        Key::H => "h",
        Key::I => "i",
        Key::J => "j",
        Key::K => "k",
        Key::L => "l",
        Key::M => "m",
        Key::N => "n",
        Key::O => "o",
        Key::P => "p",
        Key::Q => "q",
        Key::R => "r",
        Key::S => "s",
        Key::T => "t",
        Key::U => "u",
        Key::V => "v",
        Key::W => "w",
        Key::X => "x",
        Key::Y => "y",
        Key::Z => "z",
        Key::Digit0 => "0",
        Key::Digit1 => "1",
        Key::Digit2 => "2",
        Key::Digit3 => "3",
        Key::Digit4 => "4",
        Key::Digit5 => "5",
        Key::Digit6 => "6",
        Key::Digit7 => "7",
        Key::Digit8 => "8",
        Key::Digit9 => "9",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        Key::Delete => "Delete",
        Key::Insert => "Insert",
        Key::Home => "Home",
        Key::End => "End",
        Key::Tab => "Tab",
        Key::Escape => "Escape",
        Key::PageUp => "Page_Up",
        Key::PageDown => "Page_Down",
        Key::Backspace => "BackSpace",
        Key::Enter => "Return",
        Key::Space => "space",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Num0 => "KP_0",
        Key::Num1 => "KP_1",
        Key::Num2 => "KP_2",
        Key::Num3 => "KP_3",
        Key::Num4 => "KP_4",
        Key::Num5 => "KP_5",
        Key::Num6 => "KP_6",
        Key::Num7 => "KP_7",
        Key::Num8 => "KP_8",
        Key::Num9 => "KP_9",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                preferred_trigger(&HotkeyCombo::parse(combo).unwrap()),
                expected,
                "wrong trigger for {combo}"
            );
        }
    }

    #[test]
    fn every_key_maps_to_a_distinct_keysym() {
        let mut seen = std::collections::HashSet::new();
        for key in Key::ALL {
            assert!(
                seen.insert(keysym_name(key)),
                "{key:?} shares keysym {}",
                keysym_name(key)
            );
        }
    }
}
