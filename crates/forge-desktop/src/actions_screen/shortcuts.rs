use super::*;
use crate::screen_shortcuts::ScreenShortcuts;

impl ActionForm {
    pub(super) fn ready(&self, cx: &App) -> bool {
        !self.name.read(cx).content().trim().is_empty() && !self.queues.is_empty()
    }
}

impl ScreenActionsView {
    fn has_open_modal(&self) -> bool {
        self.action_modal.is_some()
            || self.sub_form.is_some()
            || self.add_trigger.is_some()
            || self.grid_picker.is_some()
            || self.history_modal.is_some()
            || self.test_run.is_some()
            || self.pending_delete.get().is_some()
    }
}

impl ScreenShortcuts for ScreenActionsView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.sub_form.clone() {
            form.update(cx, |form, cx| form.submit(cx));
            return;
        }
        if matches!(self.add_trigger, Some(AddTriggerStage::Fill(_))) {
            self.submit_trigger_fill(cx);
            return;
        }
        if self
            .action_modal
            .as_ref()
            .is_some_and(|form| form.ready(cx))
        {
            self.submit_action_modal(cx);
        }
    }

    fn run_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.has_open_modal() {
            return;
        }
        self.start_test_run(cx);
    }

    fn new_action_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_open_modal() {
            return;
        }
        self.open_action_modal(None, window, cx);
    }
}
