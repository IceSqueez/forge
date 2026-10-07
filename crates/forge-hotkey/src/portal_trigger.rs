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
