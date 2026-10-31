use super::*;
use crate::screen_shortcuts::ScreenShortcuts;

impl ScreenActionsView {
    pub(super) fn has_open_modal(&self, cx: &App) -> bool {
        self.screen_modal_open(cx) || self.editor.read(cx).has_open_modal()
    }
}

impl ScreenShortcuts for ScreenActionsView {
    fn save_shortcut(&mut self, cx: &mut Context<Self>) {
        if self
            .editor
            .update(cx, |editor, cx| editor.save_open_form(cx))
        {
            return;
        }
        let Some(form) = self
            .modals
            .action_form
            .as_ref()
            .map(|host| host.view.clone())
        else {
            return;
        };
        if form.read(cx).ready(cx) {
            form.update(cx, |form, cx| form.submit(cx));
        }
    }

    fn run_shortcut(&mut self, cx: &mut Context<Self>) {
        if self.has_open_modal(cx) {
            return;
        }
        self.editor
            .update(cx, |editor, cx| editor.start_test_run(cx));
    }

    fn new_action_shortcut(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.has_open_modal(cx) {
            return;
        }
        self.open_action_form(None, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_storage::MockActionRepo;
    use forge_storage::MockMediaRepo;
    use gpui::{TestAppContext, VisualTestContext};

    use super::super::editor::tests::view_with;
    use super::*;
    use crate::actions_screen::editor::integration_gating::tests::detail;
    use crate::test_support::{error_toasts, runtime};

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
            crate::test_support::settle_runtime_work(cx, &self.rt);
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
                let form = view.modals.action_form.as_ref().unwrap().view.clone();
                form.update(cx, |form, cx| form.fill(name, with_queue, cx));
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
            self.view.update(cx, |view, cx| {
                let detail = detail(Vec::new());
                view.tree
                    .update(cx, |tree, _| tree.set_selected(Some(detail.action.id)));
                view.editor
                    .update(cx, |editor, _| editor.show_detail(detail));
            });
        }

        fn ask_delete(&self, cx: &mut TestAppContext) {
            self.view.update(cx, |view, cx| {
                view.tree
                    .update(cx, |tree, _| tree.ask_delete(ActionId::new()));
                view.sync_step_drag_lock(cx);
            });
        }

        fn form_open(&self, cx: &mut TestAppContext) -> bool {
            self.view
                .read_with(cx, |view, _| view.modals.action_form.is_some())
        }

        fn test_run_open(&self, cx: &mut TestAppContext) -> bool {
            self.view.read_with(cx, |view, cx| {
                view.editor.read(cx).test_run_modal().is_some()
            })
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
            view.modals.action_form.as_ref().map(|host| {
                let form = host.view.read(cx);
                (form.editing_id(), form.name_text(cx))
            })
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
        let first = screen
            .view
            .read_with(cx, |view, cx| view.editor.read(cx).test_run_modal());

        screen.press_run(cx);

        let second = screen
            .view
            .read_with(cx, |view, cx| view.editor.read(cx).test_run_modal());
        assert_eq!(second.map(|m| m.entity_id()), first.map(|m| m.entity_id()));
    }
}
