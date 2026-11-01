use forge_hotkey::HotkeyCombo;
use gpui::Keystroke;

pub(crate) fn keystroke_to_combo(keystroke: &Keystroke) -> Option<String> {
    let key = keystroke.key.as_str();
    if key.is_empty() {
        return None;
    }
    let modifiers = &keystroke.modifiers;
    let mut parts: Vec<&str> = Vec::new();
    if modifiers.control {
        parts.push("Ctrl");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.platform {
        parts.push("Meta");
    }
    let raw = if parts.is_empty() {
        key.to_owned()
    } else {
        format!("{}+{key}", parts.join("+"))
    };
    HotkeyCombo::parse(&raw).ok().map(|c| c.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use gpui::Modifiers;

    use super::*;

    fn keystroke(modifiers: Modifiers, key: &str) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        }
    }

    #[test]
    fn keystroke_to_combo_maps_each_gpui_modifier_flag_to_its_own_combo_token() {
        let cases = [
            (Modifiers::default(), "A"),
            (
                Modifiers {
                    control: true,
                    ..Default::default()
                },
                "Ctrl+A",
            ),
            (
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                "Shift+A",
            ),
            (
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
                "Alt+A",
            ),
            (
                Modifiers {
                    platform: true,
                    ..Default::default()
                },
                "Meta+A",
            ),
            (
                Modifiers {
                    function: true,
                    ..Default::default()
                },
                "A",
            ),
            (
                Modifiers {
                    control: true,
                    shift: true,
                    alt: true,
                    platform: true,
                    function: false,
                },
                "Ctrl+Shift+Alt+Meta+A",
            ),
        ];

        for (modifiers, expected) in cases {
            assert_eq!(
                keystroke_to_combo(&keystroke(modifiers, "a")).as_deref(),
                Some(expected),
                "wrong combo for {modifiers:?}"
            );
        }
    }

    #[test]
    fn keystroke_to_combo_rejects_keystrokes_the_combo_grammar_cannot_express() {
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        let cases = [
            (Modifiers::default(), ""),
            (ctrl, ""),
            (ctrl, "f13"),
            (ctrl, ";"),
            (Modifiers::default(), "shift"),
            (ctrl, "shift"),
        ];

        for (modifiers, key) in cases {
            assert_eq!(
                keystroke_to_combo(&keystroke(modifiers, key)),
                None,
                "expected no combo for key {key:?} with {modifiers:?}"
            );
        }
    }
}
