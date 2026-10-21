use gpui::Context;

use super::OverlaysView;
use super::editor_state::EditorMode;
use crate::screen_shortcuts::ScreenShortcuts;
use crate::unsaved_work::UnsavedWork;

impl ScreenShortcuts for OverlaysView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.pending_delete.get().is_some() {
            return;
        }
        if let Some(form) = self.form.as_ref().map(|open| open.view.clone()) {
            form.update(cx, |form, cx| form.submit(cx));
            return;
        }
        match self.editor.mode {
            EditorMode::Code => {
                self.start_saving_unsaved_work(cx);
            }
            EditorMode::Design => {
                if let Some(panel) = self.editor.panel.as_ref().map(|open| open.view.clone()) {
                    panel.update(cx, |panel, cx| panel.emit_save(cx));
                }
            }
        }
    }
}
