use forge_components::{BORDER_THIN, ForgePalette, Icon, ToastKind, body_family, icon, tr};
use forge_types::TriggerInstanceId;
use gpui::{AnyElement, ClickEvent, Context, Keystroke, Pixels, div, prelude::*, px};

use super::HotkeysScreenView;
use super::conflict::ConflictPrompt;
use crate::actions::shortcut_entry;
use crate::combo_capture::{CapturedKey, captured_key, warn_if_typing_key};
use crate::combo_conflict::{Claimant, holder_of_combo};
use crate::shortcut_overrides::ChordVerdict;
use crate::toasts::PushToast;

const ADD_BAR_PAD_H: Pixels = px(12.0);

const ADD_BAR_RADIUS: Pixels = px(9.0);

const ADD_BAR_FS: Pixels = px(12.0);

const CAPTURE_PAD_V: Pixels = px(11.0);
const CAPTURE_GAP: Pixels = px(8.0);
const CAPTURE_GLYPH: Pixels = px(14.0);
const CAPTURE_CANCEL_ML: Pixels = px(6.0);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Capture {
    Off,
    Add,
    Rebind(TriggerInstanceId),
    Modal(Option<TriggerInstanceId>),
    App(&'static str),
    AppModal(&'static str),
}

impl Capture {
    pub(super) fn target(self) -> Option<TriggerInstanceId> {
        match self {
            Capture::Rebind(id) | Capture::Modal(Some(id)) => Some(id),
            _ => None,
        }
    }

    fn claimant(self) -> Claimant {
        match self.target() {
            Some(id) => Claimant::Trigger(id),
            None => Claimant::NewTrigger,
        }
    }
}

impl HotkeysScreenView {
    pub(super) fn start_capture(&mut self, capture: Capture, cx: &mut Context<Self>) {
        self.capture = capture;
        self.menu_open = None;
        self.key_capture.start(cx, Self::on_capture_keystroke);
        cx.notify();
    }

    fn on_capture_keystroke(&mut self, keystroke: Keystroke, cx: &mut Context<Self>) -> bool {
        if self.capture == Capture::Off {
            return false;
        }
        let key = captured_key(&keystroke);
        if key == CapturedKey::Cancel {
            self.cancel_capture(cx);
            return true;
        }
        if let Capture::App(id) | Capture::AppModal(id) = self.capture {
            self.on_app_capture(id, &keystroke, cx);
            return true;
        }
        if let CapturedKey::Combo(combo) = key {
            self.on_capture_combo(combo, cx);
        }
        true
    }

    fn on_app_capture(&mut self, id: &'static str, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let capture = self.capture;
        match self.shortcuts.verdict(keystroke, id) {
            ChordVerdict::Unusable => {}
            ChordVerdict::NeedsModifier => {
                self.end_capture();
                self.stop_app_modal_capture(cx);
                cx.push_toast(
                    ToastKind::Warn,
                    tr!("settings_shortcuts_error_needs_modifier"),
                );
                cx.notify();
            }
            ChordVerdict::Taken { owner_id, chord } => {
                self.end_capture();
                self.conflict = Some(ConflictPrompt {
                    combo: chord,
                    holder: shortcut_entry(owner_id)
                        .map(|entry| tr!(entry.label_key))
                        .unwrap_or_default(),
                    capture,
                    app_owner: Some(owner_id),
                    taken_by: None,
                });
                cx.notify();
            }
            ChordVerdict::Free(chord) => {
                self.end_capture();
                self.apply_capture(capture, chord, cx);
            }
        }
    }

    pub(super) fn end_capture(&mut self) {
        self.capture = Capture::Off;
        self.key_capture.stop();
    }

    pub(super) fn stop_app_modal_capture(&mut self, cx: &mut Context<Self>) {
        if let Some(open) = &self.app_modal {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
    }

    fn cancel_capture(&mut self, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.key_capture.stop();
        if matches!(capture, Capture::Modal(_))
            && let Some(open) = &self.modal
        {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
        if matches!(capture, Capture::AppModal(_)) {
            self.stop_app_modal_capture(cx);
        }
        cx.notify();
    }

    fn on_capture_combo(&mut self, combo: String, cx: &mut Context<Self>) {
        let capture = std::mem::replace(&mut self.capture, Capture::Off);
        self.key_capture.stop();
        warn_if_typing_key(&combo, cx);
        match holder_of_combo(&self.bindings, &self.clip_keys, &combo, capture.claimant()) {
            Some(holder) => {
                self.conflict = Some(ConflictPrompt {
                    combo,
                    holder: holder.label(),
                    capture,
                    app_owner: None,
                    taken_by: Some(holder),
                });
                cx.notify();
            }
            None => self.apply_capture(capture, combo, cx),
        }
    }

    pub(super) fn apply_capture(
        &mut self,
        capture: Capture,
        combo: String,
        cx: &mut Context<Self>,
    ) {
        match capture {
            Capture::Off => cx.notify(),
            Capture::Add => self.open_add_modal(combo, cx),
            Capture::Rebind(key) => self.rebind(key, combo, cx),
            Capture::Modal(editing) => {
                let lock = match editing {
                    Some(_) => None,
                    None => self.locked_edge_for(&combo),
                };
                if let Some(open) = &self.modal {
                    open.view.update(cx, |modal, cx| {
                        if editing.is_none() {
                            modal.set_edge_lock(lock, cx);
                        }
                        modal.apply_capture(combo, cx);
                    });
                }
                cx.notify();
            }
            Capture::App(id) => {
                self.shortcuts.bind(id, combo);
                self.persist_shortcuts(cx);
            }
            Capture::AppModal(_) => self.push_app_modal_chord(combo, None, cx),
        }
    }

    pub(super) fn render_capture_row(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cancel_hover = palette.text_secondary;
        div()
            .id("hotkeys-capture-row")
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(CAPTURE_GAP)
            .py(CAPTURE_PAD_V)
            .px(ADD_BAR_PAD_H)
            .rounded(ADD_BAR_RADIUS)
            .border(BORDER_THIN)
            .border_color(palette.success)
            .bg(palette.surface_overlay)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_capture(cx)))
            .child(icon(Icon::Keyboard, CAPTURE_GLYPH, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.success)
                    .child(tr!("hotkeys_capture_prompt")),
            )
            .child(
                div()
                    .id("hotkeys-capture-cancel")
                    .ml(CAPTURE_CANCEL_ML)
                    .flex()
                    .items_center()
                    .font_family(body_family())
                    .text_size(ADD_BAR_FS)
                    .text_color(palette.text_faint)
                    .cursor_pointer()
                    .hover(move |style| style.text_color(cancel_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.cancel_capture(cx);
                    }))
                    .child(tr!("common_cancel")),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_claims_as_the_binding_a_capture_must_not_conflict_with() {
        let id = TriggerInstanceId::new();
        let cases = [
            ("idle", Capture::Off, Claimant::NewTrigger),
            ("adding a new binding", Capture::Add, Claimant::NewTrigger),
            (
                "rebinding from the row menu",
                Capture::Rebind(id),
                Claimant::Trigger(id),
            ),
            (
                "recapturing inside an edit modal",
                Capture::Modal(Some(id)),
                Claimant::Trigger(id),
            ),
            (
                "recapturing inside an add modal",
                Capture::Modal(None),
                Claimant::NewTrigger,
            ),
        ];

        for (case, capture, expected) in cases {
            assert_eq!(capture.claimant(), expected, "wrong claimant while {case}");
        }
    }
}
