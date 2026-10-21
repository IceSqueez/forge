use gpui::Context;

use super::TriggersRegistryView;
use super::create::CreateStage;
use crate::screen_shortcuts::ScreenShortcuts;

impl ScreenShortcuts for TriggersRegistryView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        match self.create {
            Some(CreateStage::Fill(_)) => self.submit_create(cx),
            Some(_) => {}
            None => {
                if self.pending_delete.get().is_none() {
                    self.commit_config(cx);
                    cx.notify();
                }
            }
        }
    }
}
