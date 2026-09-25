use forge_components::{ToastKind, tr};
use forge_hotkey::HotkeyCombo;
use gpui::{App, Context, Keystroke, Subscription};

use crate::hotkey_bindings::keystroke_to_combo;
use crate::toasts::PushToast;

const ESCAPE_KEY: &str = "escape";

#[derive(Debug, PartialEq, Eq)]
pub enum CapturedKey {
    Cancel,
    Combo(String),
    /// A keystroke the combo grammar cannot express; capture keeps listening.
    Unusable,
}

pub fn captured_key(keystroke: &Keystroke) -> CapturedKey {
    if keystroke.key == ESCAPE_KEY && !keystroke.modifiers.modified() {
        return CapturedKey::Cancel;
    }
    match keystroke_to_combo(keystroke) {
        Some(combo) => CapturedKey::Combo(combo),
        None => CapturedKey::Unusable,
    }
}

#[derive(Default)]
pub struct ComboCapture {
    intercept: Option<Subscription>,
}

impl ComboCapture {
    /// The handler returns whether it consumed the keystroke.
    pub fn start<V: 'static>(
        &mut self,
        cx: &mut Context<V>,
        on_key: impl Fn(&mut V, Keystroke, &mut Context<V>) -> bool + 'static,
    ) {
        let weak = cx.entity().downgrade();
        self.intercept = Some(cx.intercept_keystrokes(move |event, _window, cx| {
            let keystroke = event.keystroke.clone();
            let handled = weak
                .update(cx, |view, cx| on_key(view, keystroke, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
    }

    pub fn stop(&mut self) {
        self.intercept = None;
    }

    pub fn is_active(&self) -> bool {
        self.intercept.is_some()
    }
}

pub fn warn_if_typing_key(combo: &str, cx: &mut App) {
    if HotkeyCombo::parse(combo).is_ok_and(|parsed| parsed.swallows_typing()) {
        cx.push_toast(
            ToastKind::Warn,
            tr!("hotkeys_toast_bare_key_global", combo = combo),
        );
    }
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
    fn only_a_bare_escape_cancels_capture() {
        for (stroke, expected) in [
            (
                keystroke(Modifiers::default(), ESCAPE_KEY),
                CapturedKey::Cancel,
            ),
            (
                keystroke(Modifiers::shift(), ESCAPE_KEY),
                CapturedKey::Combo("Shift+Escape".to_owned()),
            ),
            (
                keystroke(Modifiers::control(), "f9"),
                CapturedKey::Combo("Ctrl+F9".to_owned()),
            ),
            (keystroke(Modifiers::default(), ""), CapturedKey::Unusable),
        ] {
            assert_eq!(captured_key(&stroke), expected, "{stroke:?}");
        }
    }
}
