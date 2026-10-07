use std::sync::Arc;

use forge_components::ForgePalette;
use gpui::{AnyElement, Context};

use super::HotkeysScreenView;
use super::capture::Capture;
use crate::actions::chord_caps;
use crate::async_bridge;
use crate::combo_conflict::{ComboHolder, conflict_prompt, release_holder};

pub(super) struct ConflictPrompt {
    pub(super) combo: String,
    pub(super) holder: String,
    pub(super) capture: Capture,
    pub(super) app_owner: Option<&'static str>,
    pub(super) taken_by: Option<ComboHolder>,
}

impl HotkeysScreenView {
    fn conflict_replace(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.conflict.take() else {
            return;
        };
        let ConflictPrompt {
            combo,
            capture,
            app_owner,
            taken_by,
            ..
        } = prompt;
        if let Some(owner) = app_owner {
            if matches!(capture, Capture::AppModal(_)) {
                self.push_app_modal_chord(combo, Some(owner), cx);
            } else {
                self.shortcuts.unbind(owner);
                self.apply_capture(capture, combo, cx);
            }
            return;
        }
        let Some(holder) = taken_by else {
            self.apply_capture(capture, combo, cx);
            return;
        };
        let reconciler = Arc::clone(&self.reconciler);
        let backend = Arc::clone(&self.backend);
        let doomed = combo.clone();
        async_bridge::run_async(
            &self.rt_handle,
            release_holder(holder, doomed, reconciler, backend),
            move |this, result: Result<(), String>, cx| match result {
                Ok(()) => {
                    if capture.target().is_none() {
                        this.load(cx);
                    }
                    this.apply_capture(capture, combo, cx);
                }
                Err(message) => this.on_repo_error(&message, cx),
            },
            cx,
        );
        cx.notify();
    }

    fn conflict_cancel(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.conflict.take() else {
            return;
        };
        if matches!(prompt.capture, Capture::Modal(_))
            && let Some(open) = &self.modal
        {
            open.view.update(cx, |modal, cx| modal.cancel_capture(cx));
        }
        if matches!(prompt.capture, Capture::AppModal(_)) {
            self.stop_app_modal_capture(cx);
        }
        cx.notify();
    }

    pub(super) fn render_conflict(
        &self,
        prompt: &ConflictPrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item_name = match prompt.capture {
            Capture::App(_) => chord_caps(&prompt.combo),
            _ => prompt.combo.clone(),
        };
        conflict_prompt(
            "hotkeys",
            item_name,
            &prompt.holder,
            palette,
            cx,
            Self::conflict_cancel,
            Self::conflict_replace,
        )
    }
}
