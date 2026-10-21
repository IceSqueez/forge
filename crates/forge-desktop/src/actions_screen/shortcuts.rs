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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_storage::MockMediaRepo;
    use forge_storage::action::MockActionRepo;
    use gpui::{TestAppContext, VisualTestContext};

    use super::super::editor::tests::view_with;
    use super::super::integration_gating::tests::detail;
    use super::*;
    use crate::test_support::{error_toasts, pump, runtime};

    struct Hosting(Entity<ScreenActionsView>);

    impl Render for Hosting {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.0.clone())
        }
    }

    struct Rig {
        view: Entity<ScreenActionsView>,
        saves: Arc<AtomicUsize>,
        rt: tokio::runtime::Runtime,
    }

    impl Rig {
        fn mount(cx: &mut TestAppContext) -> Self {
            crate::i18n::install_language(forge_storage::Language::En);
            cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
            let rt = runtime();
            let saves = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&saves);
            let mut repo = MockActionRepo::new();
            repo.expect_list().returning(|| Ok(Vec::new()));
            repo.expect_get().returning(|_| Ok(None));
            repo.expect_save().returning(move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
            let view = view_with(
                cx,
                &rt,
                Vec::new(),
                MockMediaRepo::new(),
                SubActionRegistry::new(),
                Arc::new(repo),
            );
            let screen = Self { view, saves, rt };
            screen.settle(cx);
            screen
        }

        fn settle(&self, cx: &mut TestAppContext) {
            for _ in 0..3 {
                pump(&self.rt);
                cx.run_until_parked();
            }
        }

        fn press_new_action(&self, cx: &mut TestAppContext) {
            let hosted = self.view.clone();
            let window = cx.add_window(|_, _| Hosting(hosted));
            let vcx = &mut VisualTestContext::from_window(window.into(), cx);
            vcx.update(|window, cx| {
                self.view
                    .update(cx, |view, cx| view.new_action_shortcut(window, cx));
            });
            self.settle(cx);
        }

        fn fill_action_form(&self, cx: &mut TestAppContext, name: &str, with_queue: bool) {
            let name = name.to_owned();
            self.view.update(cx, |view, cx| {
                let form = view.action_modal.as_mut().unwrap();
                form.name
                    .update(cx, |input, cx| input.set_content(name, cx));
                if with_queue {
                    form.queues = vec![(QueueId::new(), "Default".into())];
                }
            });
        }

        fn press_save(&self, cx: &mut TestAppContext) {
            self.view.update(cx, |view, cx| view.save_shortcut(cx));
            self.settle(cx);
        }

        fn press_run(&self, cx: &mut TestAppContext) {
            self.view.update(cx, |view, cx| view.run_shortcut(cx));
            self.settle(cx);
        }

        fn open_action(&self, cx: &mut TestAppContext) {
            self.view.update(cx, |view, _| {
                let detail = detail(Vec::new());
                view.selected = Some(detail.action.id);
                view.detail = Some(detail);
            });
        }

        fn ask_delete(&self, cx: &mut TestAppContext) {
            self.view
                .update(cx, |view, _| view.pending_delete.request(ActionId::new()));
        }

        fn form_open(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.action_modal.is_some())
        }

        fn test_run_open(&self, cx: &mut TestAppContext) -> bool {
            self.view.read_with(cx, |view, _| view.test_run.is_some())
        }

        fn saved(&self) -> usize {
            self.saves.load(Ordering::SeqCst)
        }
    }

    #[gpui::test]
    fn the_new_action_shortcut_opens_a_blank_action_form(cx: &mut TestAppContext) {
        let screen = Rig::mount(cx);

        screen.press_new_action(cx);

        let blank = screen.view.read_with(cx, |view, cx| {
            view.action_modal
                .as_ref()
                .map(|form| (form.editing, form.name.read(cx).content().to_owned()))
        });
        assert_eq!(blank, Some((None, String::new())));
    }

    #[gpui::test]
    fn the_new_action_shortcut_under_a_delete_prompt_opens_nothing(cx: &mut TestAppContext) {
        let screen = Rig::mount(cx);
        screen.ask_delete(cx);

        screen.press_new_action(cx);

        assert!(!screen.form_open(cx));
    }

    #[gpui::test]
    fn the_save_shortcut_stores_an_action_form_only_when_its_create_button_is_enabled(
        cx: &mut TestAppContext,
    ) {
        for (name, with_queue, stored, case) in [
            ("Hydrate", true, true, "named with a queue"),
            ("", true, false, "blank name"),
            ("   ", true, false, "whitespace name"),
            ("Hydrate", false, false, "no queue listed yet"),
        ] {
            let screen = Rig::mount(cx);
            screen.press_new_action(cx);
            screen.fill_action_form(cx, name, with_queue);

            screen.press_save(cx);

            assert_eq!(
                (screen.saved(), screen.form_open(cx)),
                (usize::from(stored), !stored),
                "{case}"
            );
        }
    }

    #[gpui::test]
    fn the_save_shortcut_without_a_queue_raises_no_error_toast(cx: &mut TestAppContext) {
        let screen = Rig::mount(cx);
        screen.press_new_action(cx);
        screen.fill_action_form(cx, "Hydrate", false);

        screen.press_save(cx);

        assert_eq!(cx.update(|cx| error_toasts(cx)), 0);
    }

    #[gpui::test]
    fn the_save_shortcut_with_no_dialog_open_stores_nothing(cx: &mut TestAppContext) {
        let screen = Rig::mount(cx);
        screen.open_action(cx);

        screen.press_save(cx);

        assert_eq!(screen.saved(), 0);
    }

    #[gpui::test]
    fn the_run_shortcut_opens_a_test_run_only_for_an_open_action_with_no_dialog(
        cx: &mut TestAppContext,
    ) {
        for (open, delete_prompt, runs, case) in [
            (true, false, true, "open action"),
            (true, true, false, "delete prompt showing"),
            (false, false, false, "no action open"),
        ] {
            let screen = Rig::mount(cx);
            if open {
                screen.open_action(cx);
            }
            if delete_prompt {
                screen.ask_delete(cx);
            }

            screen.press_run(cx);

            assert_eq!(screen.test_run_open(cx), runs, "{case}");
        }
    }

    #[gpui::test]
    fn the_run_shortcut_while_a_test_run_is_showing_keeps_that_run(cx: &mut TestAppContext) {
        let screen = Rig::mount(cx);
        screen.open_action(cx);
        screen.press_run(cx);
        let first = screen.view.read_with(cx, |view, _| view.test_run.clone());

        screen.press_run(cx);

        let second = screen.view.read_with(cx, |view, _| view.test_run.clone());
        assert_eq!(second.map(|m| m.entity_id()), first.map(|m| m.entity_id()));
    }
}
