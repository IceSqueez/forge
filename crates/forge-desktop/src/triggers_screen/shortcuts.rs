use gpui::Context;

use super::TriggersRegistryView;
use crate::screen_shortcuts::ScreenShortcuts;

impl ScreenShortcuts for TriggersRegistryView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.create.read(cx).is_filling() {
            self.create.update(cx, |flow, cx| flow.submit_create(cx));
        } else if !self.create.read(cx).is_open() && !self.list.read(cx).delete_pending() {
            self.pane.update(cx, |pane, cx| {
                pane.commit_config(cx);
                cx.notify();
            });
        }
    }
}
